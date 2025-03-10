use byteyarn::{yarn, Yarn};
use ruff_python_ast::{Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign, StmtClassDef, StmtMatch, StmtRaise, StmtTry, StmtTypeAlias, StmtWith};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::{trace, warn};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use crate::{constants::*, oyarn, Sy};
use crate::core::symbols::symbol::Symbol;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak, EvaluationValue};
use super::file_mgr::{FileInfo, FileMgr};
use super::python_arch_builder::PythonArchBuilder;
use super::python_arch_eval::PythonArchEval;

#[derive(Debug)]
pub struct PythonValidator {
    entry_point: Rc<RefCell<EntryPoint>>,
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
    pub fn new(entry_point: Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) -> Self {
        Self {
            entry_point,
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
        let file_info_rc = odoo.get_file_mgr().borrow().get_file_info(&path).expect("File not found in cache").clone();
        file_info_rc
    }

    /* Validate the symbol. The dependencies must be done before any validation. */
    pub fn validate(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack[0].borrow();
        self.current_module = symbol.find_module();
        if symbol.build_status(BuildSteps::VALIDATION) != BuildStatus::PENDING {
            return;
        }
        let sym_type = symbol.typ().clone();
        drop(symbol);
        let file_info_rc = self.get_file_info(session.sync_odoo).clone();
        match sym_type {
            SymType::FILE | SymType::PACKAGE(_) => {
                if self.sym_stack[0].borrow().build_status(BuildSteps::ODOO) != BuildStatus::DONE {
                    return;
                }
                if DEBUG_STEPS {
                trace!("Validating {}", self.sym_stack[0].borrow().paths().first().unwrap_or(&S!("No path found")));
                }
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                file_info_rc.borrow_mut().replace_diagnostics(BuildSteps::VALIDATION, vec![]);
                let file_info = file_info_rc.borrow();
                if file_info_rc.borrow().text_hash != self.sym_stack[0].borrow().get_processed_text_hash(){
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::INVALID);
                    return;
                }
                if file_info.ast.is_some() && file_info.valid {
                    self.validate_body(session, file_info.ast.as_ref().unwrap());
                }
                drop(file_info);
                let mut file_info = file_info_rc.borrow_mut();
                file_info.replace_diagnostics(BuildSteps::VALIDATION, self.diagnostics.clone());
            },
            SymType::FUNCTION => {
                if DEBUG_STEPS {
                trace!("Validating function {}", self.sym_stack[0].borrow().name());
                }
                self.file_mode = false;
                let func = &self.sym_stack[0];
                let Some(parent_file) = func.borrow().get_file().and_then(|parent_weak| parent_weak.upgrade()) else {
                    panic!("Parent file not found on validating function")
                };
                if file_info_rc.borrow().text_hash != parent_file.borrow().get_processed_text_hash(){
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::INVALID);
                    return;
                }
                if func.borrow().as_func().arch_status == BuildStatus::PENDING { //TODO other checks to do? maybe odoo step, or?????????
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::PENDING);
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
                    let mut builder = PythonArchBuilder::new(self.entry_point.clone(), func.clone());
                    builder.load_arch(session);
                }
                if func.borrow().as_func().arch_eval_status == BuildStatus::PENDING { //TODO other checks to do? maybe odoo step, or?????????
                    let mut builder = PythonArchEval::new(self.entry_point.clone(), func.clone());
                    builder.eval_arch(session);
                }
                if func.borrow().as_func().arch_eval_status != BuildStatus::DONE {
                    return;
                }
                self.diagnostics = vec![];
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
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
                FileMgr::delete_path(session, &symbol.paths()[0].to_string());
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
                    let sym = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&OYarn::from(f.name.to_string()), &f.range);
                    if let Some(sym) = sym {
                        let val_status = sym.borrow().build_status(BuildSteps::VALIDATION).clone();
                        if val_status == BuildStatus::PENDING {
                            let mut v = PythonValidator::new(self.entry_point.clone(), sym.clone());
                            v.validate(session);
                        } else if val_status == BuildStatus::IN_PROGRESS {
                            panic!("cyclic validation detected... Aborting");
                        }
                        self.diagnostics.extend(sym.borrow_mut().as_func_mut().diagnostics.values().flat_map(|v| v.clone()));
                    } else {
                        panic!("function '{}' not found", f.name.id);
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
                Stmt::Return(stmt_return) => self.visit_return_stmt(session, stmt_return),
                Stmt::AugAssign(stmt_aug_assign) => self.visit_aug_assign(session, stmt_aug_assign),
                Stmt::TypeAlias(stmt_type_alias) => self.visit_type_alias(session, stmt_type_alias),
                Stmt::With(stmt_with) => self.visit_with(session, stmt_with),
                Stmt::Match(stmt_match) => self.visit_match(session, stmt_match),
                Stmt::Raise(stmt_raise) => self.visit_raise(session, stmt_raise),
                Stmt::Assert(stmt_assert) => self.visit_assert(session, stmt_assert),
                Stmt::Global(_) => {},
                Stmt::Nonlocal(_) => {},
                Stmt::Pass(_) => {},
                Stmt::IpyEscapeCommand(_) => {},
            }
        }
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, c: &StmtClassDef) {
        let sym = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&OYarn::from(c.name.to_string()), &c.range);
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
        for alias in name_aliases.iter() {
            if alias.name.id == "*" {
                continue;
            }
            if self.current_module.is_some() {
                let var_name = if alias.asname.is_none() {
                    S!(alias.name.split(".").next().unwrap())
                } else {
                    alias.asname.as_ref().unwrap().clone().to_string()
                };
                let variable = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&OYarn::from(var_name), &alias.range);
                if let Some(variable) = variable {
                    for evaluation in variable.borrow().evaluations().as_ref().unwrap().iter() {
                        let eval_sym = evaluation.symbol.get_symbol(session, &mut None, &mut self.diagnostics, Some(file_symbol.clone()));
                        match eval_sym {
                            EvaluationSymbolPtr::WEAK(w) => {
                                if let Some(symbol) = w.weak.upgrade() {
                                    let module = symbol.borrow().find_module();
                                    if let Some(module) = module {
                                        if !ModuleSymbol::is_in_deps(session, self.current_module.as_ref().unwrap(), &module.borrow().as_module_package().dir_name) && !self.safe_imports.last().unwrap() {
                                            self.diagnostics.push(Diagnostic::new(
                                                Range::new(Position::new(alias.range.start().to_u32(), 0), Position::new(alias.range.end().to_u32(), 0)),
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
                            },
                            _ => {
                                panic!("Internal error: evaluated has invalid evaluationType");
                            }
                        }
                    }
                }
            }
        }
    }

    fn visit_aug_assign(&mut self, session: &mut SessionInfo, assign: &StmtAugAssign) {
        self.validate_expr(session, &assign.value, &assign.range.start());
    }

    fn visit_ann_assign(&mut self, session: &mut SessionInfo, assign: &StmtAnnAssign) {
        if let Some(value) = assign.value.as_ref() {
            self.validate_expr(session, value, &assign.range.start());
        }
    }

    fn visit_assign(&mut self, session: &mut SessionInfo, assign: &StmtAssign) {
        self.validate_expr(session, &assign.value, &assign.range.start());
    }

    fn visit_with(&mut self, session: &mut SessionInfo, stmt_with: &StmtWith) {
        for item in stmt_with.items.iter() {
            self.validate_expr(session, &item.context_expr, &stmt_with.range.start());
        }
        self.validate_body(session, &stmt_with.body);
    }

    fn _check_model(&mut self, session: &mut SessionInfo, class: &Rc<RefCell<Symbol>>) {
        let class_ref = class.borrow();
        let Some(model_data) = class_ref.as_class_sym()._model.as_ref() else {
            return;
        };
        if self.current_module.is_none() {
            return;
        }
        let maybe_from_module = class_ref.find_module();
        // Check fields, check related and comodel arguments
        for symbol in class_ref.all_symbols(){
            let sym_ref = symbol.borrow();
            if sym_ref.typ() != SymType::VARIABLE {
                continue;
            }
            let Some(evals) = sym_ref.evaluations() else {
                continue;
            };
            for eval in evals.iter() {
                let symbol = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
                let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None, &mut vec![]);
                for eval_weak in eval_weaks.iter() {
                    let Some(symbol) = eval_weak.upgrade_weak() else {continue};
                    if !symbol.borrow().is_field_class(session){
                        continue;
                    }
                    if let Some(related_field_name) = eval_weak.as_weak().context.get(&S!("related")).map(|ctx_val| ctx_val.as_string()) {
                        let Some(special_arg_range) = eval_weak.as_weak().context.get(&S!("special_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                            continue;
                        };
                        let syms = PythonArchEval::get_nested_sub_field(session, &related_field_name, class.clone(), maybe_from_module.clone());
                        if syms.is_empty(){
                            self.diagnostics.push(Diagnostic::new(
                                Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                Some(DiagnosticSeverity::ERROR),
                                Some(NumberOrString::String(S!("OLS30323"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Field {related_field_name} does not exist on model {}", model_data.name),
                                None,
                                None,
                            ));
                            continue;
                        }
                        let field_type = symbol.borrow().name().clone();
                        let found_same_type_match = syms.iter().any(|sym|{
                            let related_eval_weaks = Symbol::follow_ref(&&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                                Rc::downgrade(&sym),
                                None,
                                false,
                            )), session, &mut None, true, true, None, &mut vec![]);
                            related_eval_weaks.iter().any(|related_eval_weak|{
                                let Some(related_field_class_sym) = related_eval_weak.upgrade_weak() else {
                                    return false
                                };
                                let same_field = related_field_class_sym.borrow().is_specific_field_class(session, &[field_type.as_str()]);
                                same_field
                            })
                        });
                        if !found_same_type_match{
                            self.diagnostics.push(Diagnostic::new(
                                Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                Some(DiagnosticSeverity::ERROR),
                                Some(NumberOrString::String(S!("OLS30326"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Related field is not of the same type"),
                                None,
                                None,
                            ));

                        }
                    } else if let Some(comodel_field_name) = eval_weak.as_weak().context.get(&S!("comodel")).map(|ctx_val| ctx_val.as_string()) {
                        let Some(module) = class_ref.find_module() else {
                            continue;
                        };
                        if !ModuleSymbol::is_in_deps(session, &module, &oyarn!("{}", comodel_field_name)){
                            let Some(special_arg_range) = eval_weak.as_weak().context.get(&S!("special_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", comodel_field_name)){
                                let Some(ref from_module) = maybe_from_module else {continue};
                                if !model.clone().borrow().model_in_deps(session, from_module) {
                                    self.diagnostics.push(Diagnostic::new(
                                        Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        Some(DiagnosticSeverity::ERROR),
                                        Some(NumberOrString::String(S!("OLS30324"))),
                                        Some(EXTENSION_NAME.to_string()),
                                        format!("Field comodel_name ({comodel_field_name}) is not in module dependencies"),
                                        None,
                                        None,
                                    ));
                                }
                            } else {
                                self.diagnostics.push(Diagnostic::new(
                                    Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                    Some(DiagnosticSeverity::ERROR),
                                    Some(NumberOrString::String(S!("OLS30325"))),
                                    Some(EXTENSION_NAME.to_string()),
                                    format!("Field comodel_name ({comodel_field_name}) does not exist"),
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    for special_fn_field_name in ["compute", "inverse", "search"]{
                        let Some(method_name) = eval_weak.as_weak().context.get(&S!(special_fn_field_name)).map(|ctx_val| ctx_val.as_string()) else {
                            continue;
                        };
                        let Some(module) = class_ref.find_module() else {
                            continue;
                        };
                        let (symbols, _diagnostics) = class.clone().borrow().get_member_symbol(session,
                            &method_name.to_string(),
                            Some(module.clone()),
                            false,
                            false,
                            true,
                            false
                        );
                        let method_found = symbols.iter().any(|symbol| symbol.borrow().typ() == SymType::FUNCTION);
                        if !method_found{
                            let Some(arg_range) = eval_weak.as_weak().context.get(&format!("{special_fn_field_name}_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            self.diagnostics.push(Diagnostic::new(
                                Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                Some(DiagnosticSeverity::ERROR),
                                Some(NumberOrString::String(S!("OLS30327"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Method {method_name} not found on current model"),
                                None,
                                None,
                            ));

                        }
                    }
                }
            }
        }
        //Check inherit field
        let inherit = class_ref.get_symbol(&(vec![], vec![Sy!("_inherit")]), u32::MAX);
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
            let model = session.sync_odoo.models.get(&oyarn!("{}", model));
            if let Some(model) = model {
                let model = model.clone();
                let borrowed_model = model.borrow();
                let mut main_modules = vec![];
                let mut found_one = false;
                for main_sym in borrowed_model.get_main_symbols(session, None).iter() {
                    let main_sym = main_sym.borrow();
                    let main_sym_module = main_sym.find_module();
                    if let Some(main_sym_module) = main_sym_module {
                        let module_name = main_sym_module.borrow().as_module_package().dir_name.clone();
                        main_modules.push(module_name.clone());
                        if ModuleSymbol::is_in_deps(session, from, &module_name) {
                            found_one = true;
                        }
                    }
                }
                if !found_one {
                    if !main_modules.is_empty() {
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
        let (eval, diags) = Evaluation::eval_from_ast(session, expr, self.sym_stack.last().unwrap().clone(), max_infer);
        self.diagnostics.extend(diags);
    }

    fn visit_type_alias(&mut self, session: &mut SessionInfo<'_>, stmt_type_alias: &StmtTypeAlias) {
        self.validate_expr(session, &stmt_type_alias.value, &stmt_type_alias.range.start());
    }

    fn visit_return_stmt(&mut self, session: &mut SessionInfo<'_>, stmt_return: &ruff_python_ast::StmtReturn) {
        if let Some(value) = stmt_return.value.as_ref() {
            self.validate_expr(session, value, &stmt_return.range.start());
        }
    }

    fn visit_match(&mut self, session: &mut SessionInfo<'_>, stmt_match: &StmtMatch) {
        self.validate_expr(session, &stmt_match.subject, &stmt_match.range.start());
        for case in stmt_match.cases.iter() {
            if let Some(guard) = case.guard.as_ref() {
                self.validate_expr(session, guard, &case.pattern.start());
            }
            self.validate_body(session, &case.body);
        }
    }

    fn visit_raise(&mut self, session: &mut SessionInfo<'_>, stmt_raise: &StmtRaise) {
        if let Some(exc) = stmt_raise.exc.as_ref() {
            self.validate_expr(session, exc, &stmt_raise.range.start());
        }
    }

    fn visit_assert(&mut self, session: &mut SessionInfo<'_>, stmt_assert: &StmtAssert) {
        self.validate_expr(session, &stmt_assert.test, &stmt_assert.range.start());
        if let Some(msg) = stmt_assert.msg.as_ref() {
            self.validate_expr(session, msg, &stmt_assert.range.start());
        }
    }
}
