use ruff_python_ast::{Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtTry};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::{trace, warn};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use crate::constants::*;
use crate::core::symbols::symbol::Symbol;
use crate::core::odoo::SyncOdoo;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use crate::S;

use super::evaluation::{Evaluation, EvaluationValue};
use super::file_mgr::FileInfo;
use super::python_arch_builder::PythonArchBuilder;
use super::python_arch_eval::PythonArchEval;

#[derive(Debug)]
pub struct PythonValidator {
    file_mode: bool,
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    pub diagnostics: Vec<Diagnostic>, //collect diagnostic from arch and arch_eval too from inner functions, but put everything at Validation level
    safe_imports: Vec<bool>,
    current_module: Option<Rc<RefCell<Symbol>>>
}

/* PythonValidator operate on a single Symbol. Unlike other steps, it can be done on symbol containing code (file and functions only. Not class, variable, namespace).
It will validate this node and run a validator on all subsymbol and dependencies.
It will try to inference the return type of functions if it is not annotated; */
impl PythonValidator {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> Self {
        Self {
            file_mode: true,
            sym_stack: vec![symbol],
            diagnostics: vec![],
            safe_imports: vec![false],
            current_module: None,
        }
    }

    fn get_file_info(&mut self, odoo: &mut SyncOdoo) -> Rc<RefCell<FileInfo>> {
        let file_symbol = self.sym_stack[0].borrow().get_file().unwrap().upgrade().unwrap();
        let file_symbol = file_symbol.borrow();
        let mut path = file_symbol.paths()[0].clone();
        if matches!(file_symbol.typ(), SymType::PACKAGE(_)) {
            path = PathBuf::from(path).join("__init__.py").sanitize() + file_symbol.as_package().i_ext().as_str();
        }
        let file_info_rc = odoo.get_file_mgr().borrow_mut().get_file_info(&path).expect("File not found in cache").clone();
        file_info_rc
    }

    /* Validate the symbol. The dependencies must be done before any validation. */
    pub fn validate(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack[0].borrow_mut();
        self.current_module = symbol.find_module();
        if symbol.build_status(BuildSteps::VALIDATION) != BuildStatus::PENDING {
            return;
        }
        let sym_type = symbol.typ().clone();
        drop(symbol);
        match sym_type {
            SymType::FILE | SymType::PACKAGE(_) => {
                trace!("Validating {}", self.sym_stack[0].borrow().paths().first().unwrap_or(&S!("No path found")));
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                let file_info_rc = self.get_file_info(session.sync_odoo).clone();
                file_info_rc.borrow_mut().replace_diagnostics(BuildSteps::VALIDATION, vec![]);
                let file_info = file_info_rc.borrow();
                if file_info.ast.is_some() && file_info.valid {
                    self.validate_body(session, file_info.ast.as_ref().unwrap());
                }
                drop(file_info);
                let mut file_info = file_info_rc.borrow_mut();
                file_info.replace_diagnostics(BuildSteps::VALIDATION, self.diagnostics.clone());
            },
            SymType::FUNCTION => {
                trace!("Validating function {}", self.sym_stack[0].borrow().name());
                self.file_mode = false;
                let func = &self.sym_stack[0];
                if func.borrow().as_func().arch_status == BuildStatus::PENDING { //TODO other checks to do? maybe odoo step, or?????????
                    let mut builder = PythonArchBuilder::new(func.clone());
                    builder.load_arch(session);
                }
                if func.borrow().as_func().arch_eval_status == BuildStatus::PENDING { //TODO other checks to do? maybe odoo step, or?????????
                    let mut builder = PythonArchEval::new(func.clone());
                    builder.eval_arch(session);
                }
                self.diagnostics = vec![];
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                let file_info_rc = self.get_file_info(session.sync_odoo).clone();
                let file_info = file_info_rc.borrow();
                if file_info.ast.is_some() {
                    let stmt = AstUtils::find_stmt_from_ast(file_info.ast.as_ref().unwrap(), self.sym_stack[0].borrow().ast_indexes().unwrap());
                    let body = match stmt {
                        Stmt::FunctionDef(s) => {
                            &s.body
                        },
                        _ => {panic!("Wrong statement in validation ast extraction {} ", sym_type)}
                    };
                    self.validate_body(session, body);
                    match stmt {
                        Stmt::FunctionDef(_) => {
                            self.sym_stack[0].borrow_mut().as_func_mut().diagnostics.insert(BuildSteps::VALIDATION, self.diagnostics.clone());
                        },
                        _ => {panic!("Wrong statement in validation ast extraction {} ", sym_type)}
                    }
                } else {
                    warn!("no ast found on file info");
                }
            },
            _ => {panic!("Only File, function can be validated")}
        }
        let mut symbol = self.sym_stack[0].borrow_mut();
        symbol.set_build_status(BuildSteps::VALIDATION, BuildStatus::DONE);
        if matches!(&symbol.typ(), SymType::FILE | SymType::PACKAGE(_)) {
            if !symbol.in_workspace() {
                if !symbol.is_external() {
                    return
                }
                session.sync_odoo.get_file_mgr().borrow_mut().delete_path(session, &symbol.paths()[0].to_string());
            } else {
                drop(symbol);
                let file_info = self.get_file_info(session.sync_odoo);
                let mut file_info = file_info.borrow_mut();
                file_info.publish_diagnostics(session);
            }
        }
    }

    fn validate_body(&mut self, session: &mut SessionInfo, vec_ast: &Vec<Stmt>) {
        for stmt in vec_ast.iter() {
            match stmt {
                Stmt::FunctionDef(f) => {
                    let sym = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&f.name.to_string(), &f.range);
                    if let Some(sym) = sym {
                        let val_status = sym.borrow().build_status(BuildSteps::VALIDATION).clone();
                        if val_status == BuildStatus::PENDING {
                            let mut v = PythonValidator::new(sym.clone());
                            v.validate(session);
                        } else if val_status == BuildStatus::IN_PROGRESS {
                            panic!("cyclic validation detected... Aborting");
                        }
                        self.diagnostics.extend(sym.borrow_mut().as_func_mut().diagnostics.values().flat_map(|v| v.clone()));
                    } else {
                        panic!("function not found");
                    }
                },
                Stmt::ClassDef(c) => {
                    self.visit_class_def(session, c);
                },
                Stmt::Try(t) => {
                    self.visit_try(session, t);
                },
                Stmt::Import(i) => {
                    self._resolve_import(session, None, &i.names, None, &i.range);
                },
                Stmt::ImportFrom(i) => {
                    self._resolve_import(session, i.module.as_ref(), &i.names, Some(i.level), &i.range);
                },
                Stmt::Assign(a) => {
                    self.visit_assign(session, a);
                },
                Stmt::AnnAssign(a) => {
                    self.visit_ann_assign(session, a);
                },
                Stmt::Expr(e) => {
                    self.validate_expr(session, &e.value, &e.value.start());
                },
                Stmt::If(i) => {
                    self.validate_expr(session, &i.test, &i.test.start());
                    self.validate_body(session, &i.body);
                    for elses in i.elif_else_clauses.iter() {
                        if let  Some(test) = &elses.test {
                            self.validate_expr(session, test, &test.start());
                        }
                        self.validate_body(session, &elses.body);
                    }
                },
                Stmt::Break(_) => {},
                Stmt::Continue(_) => {},
                Stmt::Delete(d) => {
                    for target in d.targets.iter() {
                        self.validate_expr(session, target, &target.start());
                    }
                },
                Stmt::For(f) => {
                    self.validate_expr(session, &f.target, &f.target.start());
                    self.validate_body(session, &f.body);
                    self.validate_body(session, &f.orelse);
                },
                Stmt::While(w) => {
                    self.validate_expr(session, &w.test, &w.test.start());
                    self.validate_body(session, &w.body);
                    self.validate_body(session, &w.orelse);
                },
                Stmt::Return(r) => {},
                _ => {
                    trace!("Stmt not handled");
                }
            }
        }
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, c: &StmtClassDef) {
        let sym = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&c.name.to_string(), &c.range);
        if let Some(sym) = sym {
            self._check_model(session, &sym);
            self.sym_stack.push(sym);
            self.validate_body(session, &c.body);
            self.sym_stack.pop();
        } else {
            //TODO panic!("symbol not found.");
        }
    }

    fn visit_try(&mut self, session: &mut SessionInfo, node: &StmtTry) {
        let mut safe_import = false;
        for handler in node.handlers.iter() {
            let handler = handler.as_except_handler().unwrap();
            if let Some(type_) = &handler.type_ {
                if type_.is_name_expr() && type_.as_name_expr().unwrap().id.to_string() == "ImportError" {
                    safe_import = true;
                }
            }
        }
        self.safe_imports.push(safe_import);
        self.validate_body(session, &node.body);
        self.safe_imports.pop();
    }

    fn _resolve_import(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) {
        let file_symbol = self.sym_stack[0].borrow().get_file();
        let file_symbol = file_symbol.expect("file symbol not found").upgrade().expect("unable to upgrade file symbol");
        let import_results = resolve_import_stmt(
            session,
            &file_symbol,
            from_stmt,
            name_aliases,
            level,
            &mut None);
        for import_result in import_results.iter() {
            if import_result.found && self.current_module.is_some() {
                let module = import_result.symbol.borrow().find_module();
                if let Some(module) = module {
                    if !ModuleSymbol::is_in_deps(session, &self.current_module.as_ref().unwrap(), &module.borrow().as_module_package().dir_name, &mut None) && !self.safe_imports.last().unwrap() {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(import_result.range.start().to_u32(), 0), Position::new(import_result.range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(S!("OLS30103"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("{} is not in the dependencies of the module", module.borrow().as_module_package().dir_name),
                            None,
                            None,
                        ))
                    }
                }
            }
        }
    }

    fn visit_ann_assign(&mut self, session: &mut SessionInfo, assign: &StmtAnnAssign) {

    }

    fn visit_assign(&mut self, session: &mut SessionInfo, assign: &StmtAssign) {

    }

    fn _check_model(&mut self, session: &mut SessionInfo, class: &Rc<RefCell<Symbol>>) {
        let cl = class.borrow();
        let Some(model) = cl.as_class_sym()._model.as_ref() else {
            return;
        };
        if self.current_module.is_none() {
            return;
        }
        //Check inherit field
        let inherit = cl.get_symbol(&(vec![], vec![S!("_inherit")]), u32::MAX);
        if let Some(inherit) = inherit.last() {
            let inherit = inherit.borrow();
            let inherit_evals = &inherit.evaluations().unwrap();
            for inherit_eval in inherit_evals.iter() {
                let inherit_value = inherit_eval.follow_ref_and_get_value(session, &mut None, &mut vec![]);
                if let Some(inherit_value) = inherit_value {
                    match inherit_value {
                        EvaluationValue::CONSTANT(Expr::StringLiteral(s)) => {
                            self._check_module_dependency(session, &s.value.to_string(), &s.range());
                        },
                        EvaluationValue::LIST(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    self._check_module_dependency(session, &s.value.to_string(), &s.range());
                                }
                            }
                        },
                        EvaluationValue::TUPLE(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    self._check_module_dependency(session, &s.value.to_string(), &s.range());
                                }
                            }
                        },
                        _ => {
                            warn!("wrong _inherit value");
                        }
                    }
                }
            }
        }
    }

    fn _check_module_dependency(&mut self, session: &mut SessionInfo, model: &String, range: &TextRange) {
        if let Some(from) = self.current_module.as_ref() {
            let model = session.sync_odoo.models.get(model);
            if let Some(model) = model {
                let model = model.clone();
                let borrowed_model = model.borrow();
                let mut main_modules = vec![];
                let mut found_one = false;
                for main_sym in borrowed_model.get_main_symbols(session, None, &mut None).iter() {
                    let main_sym = main_sym.borrow();
                    let main_sym_module = main_sym.find_module();
                    if let Some(main_sym_module) = main_sym_module {
                        let module_name = main_sym_module.borrow().as_module_package().dir_name.clone();
                        main_modules.push(module_name.clone());
                        if ModuleSymbol::is_in_deps(session, &from, &module_name, &mut None) {
                            found_one = true;
                        }
                    }
                }
                if !found_one {
                    if main_modules.len() > 0 {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(S!("OLS30104"))),
                            None,
                            S!("Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."),
                            None,
                            None)
                        )
                    } else {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(S!("OLS30102"))),
                            Some(EXTENSION_NAME.to_string()),
                            S!("Unknown model. Check your addons path"),
                            None,
                            None)
                        )
                    }
                }
            } else {
                self.diagnostics.push(Diagnostic::new(
                    Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30102"))),
                    Some(EXTENSION_NAME.to_string()),
                    S!("Unknown model. Check your addons path"),
                    None,
                    None)
                )
            }
        } else {
            //TODO do we want to raise something?
        }
    }

    fn validate_expr(&mut self, session: &mut SessionInfo, expr: &Expr, max_infer: &TextSize) {
        let (eval, diags) = Evaluation::eval_from_ast(session, &expr, self.sym_stack.last().unwrap().clone(), &max_infer);
        self.diagnostics.extend(diags);
    }
}
