use ruff_python_ast::{Alias, AnyRootNodeRef, Expr, Identifier, NodeIndex, Stmt, StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign, StmtClassDef, StmtMatch, StmtRaise, StmtTry, StmtTypeAlias, StmtWith};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::{trace, warn};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use lsp_types::{Diagnostic, Position, Range};
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::evaluation::ContextValue;
use crate::{constants::*, oyarn, Sy};
use crate::core::symbols::symbol::Symbol;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak, EvaluationValue};
use super::file_mgr::{FileInfo, FileMgr};
use super::python_arch_eval::PythonArchEval;

#[derive(Debug)]
pub struct PythonValidator {
    entry_point: Rc<RefCell<EntryPoint>>,
    file: Rc<RefCell<Symbol>>,
    file_mode: bool,
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    pub diagnostics: Vec<Diagnostic>, //collect diagnostic from arch and arch_eval too from inner functions, but put everything at Validation level
    safe_imports: Vec<bool>,
    current_module: Option<Rc<RefCell<Symbol>>>,
    file_info: Option<Rc<RefCell<FileInfo>>>,
}

/* PythonValidator operate on a single Symbol. Unlike other steps, it can be done on symbol containing code (file and functions only. Not class, variable, namespace).
It will validate this node and run a validator on all subsymbol and dependencies.
It will try to inference the return type of functions if it is not annotated; */
impl PythonValidator {
    pub fn new(entry_point: Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) -> Self {
        Self {
            entry_point,
            file: symbol.clone(), //dummy, not valid
            file_mode: true,
            sym_stack: vec![symbol],
            diagnostics: vec![],
            safe_imports: vec![false],
            current_module: None,
            file_info: None,
        }
    }

    fn get_file_info(&mut self, session: &mut SessionInfo) -> Option<Rc<RefCell<FileInfo>>> {
        let file_symbol = self.file.borrow();
        let mut path = file_symbol.paths()[0].clone();
        if matches!(file_symbol.typ(), SymType::PACKAGE(_)) {
            path = PathBuf::from(path).join("__init__.py").sanitize() + file_symbol.as_package().i_ext().as_str();
        }
        let file_info_rc = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
        let file_info_rc = match file_info_rc {
            Some(f) => f,
            None => {
                let (updated, symbol) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &file_symbol.paths()[0], None, Some(-100), true);
                if !updated {
                    warn!("File info not found for validating symbol: {} at path {}", self.sym_stack[0].borrow().name(), file_symbol.paths()[0]);
                    return None;
                }
                symbol
            }
        };
        Some(file_info_rc)
    }

    /* Validate the symbol. The dependencies must be done before any validation. */
    pub fn validate(&mut self, session: &mut SessionInfo) {
        self.file = self.sym_stack[0].borrow().get_file().unwrap().upgrade().unwrap();
        let symbol = self.sym_stack[0].borrow();
        self.current_module = symbol.find_module();
        if symbol.build_status(BuildSteps::VALIDATION) != BuildStatus::PENDING {
            return;
        }
        let sym_type = symbol.typ().clone();
        drop(symbol);
        let file_info_rc = self.get_file_info(session).clone();
        let file_info_rc = match file_info_rc {
            Some(f) => f,
            None => {
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::INVALID);
                return;
            }
        };
        self.file_info = Some(file_info_rc.clone());
        match sym_type {
            SymType::FILE | SymType::PACKAGE(_) => {
                if self.sym_stack[0].borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::DONE {
                    return;
                }
                if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !self.sym_stack[0].borrow().is_external()) {
                trace!("Validating {}", self.sym_stack[0].borrow().paths().first().unwrap_or(&S!("No path found")));
                }
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                file_info_rc.borrow_mut().replace_diagnostics(BuildSteps::VALIDATION, vec![]);
                if file_info_rc.borrow().file_info_ast.borrow().indexed_module.is_none() {
                    file_info_rc.borrow_mut().prepare_ast(session);
                }
                let file_info = file_info_rc.borrow();
                if file_info_rc.borrow().file_info_ast.borrow().text_hash != self.sym_stack[0].borrow().get_processed_text_hash(){
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::INVALID);
                    return;
                }
                if file_info.file_info_ast.borrow().indexed_module.is_some() {
                    let old_noqa = session.current_noqa.clone();
                    session.current_noqa = self.sym_stack[0].borrow().get_noqas();
                    let file_info_ast = file_info.file_info_ast.borrow();
                    self.validate_body(session, file_info_ast.get_stmts().as_ref().unwrap());
                    session.current_noqa = old_noqa;
                }
                drop(file_info);
                if self.sym_stack[0].borrow().typ() == SymType::PACKAGE(PackageType::MODULE) {
                    ModuleSymbol::validate_manifest(&self.sym_stack[0], session);
                }
                let mut file_info = file_info_rc.borrow_mut();
                file_info.replace_diagnostics(BuildSteps::VALIDATION, self.diagnostics.clone());
            },
            SymType::FUNCTION => {
                if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !self.sym_stack[0].borrow().is_external()) {
                trace!("Validating function {}", self.sym_stack[0].borrow().name());
                }
                self.file_mode = false;
                let func = &self.sym_stack[0];
                let Some(parent_file) = func.borrow().get_file().and_then(|parent_weak| parent_weak.upgrade()) else {
                    panic!("Parent file not found on validating function")
                };
                if file_info_rc.borrow().file_info_ast.borrow().text_hash != parent_file.borrow().get_processed_text_hash(){
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::INVALID);
                    return;
                }
                if func.borrow().as_func().arch_status == BuildStatus::PENDING { //TODO other checks to do? maybe odoo step, or?????????
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::PENDING);
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
                    self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
                    SyncOdoo::build_now(session, &func, BuildSteps::ARCH);
                }
                if func.borrow().as_func().arch_eval_status == BuildStatus::PENDING { //TODO other checks to do? maybe odoo step, or?????????
                    SyncOdoo::build_now(session, &func, BuildSteps::ARCH_EVAL);
                }
                if func.borrow().as_func().arch_eval_status != BuildStatus::DONE {
                    return;
                }
                self.diagnostics = vec![];
                self.sym_stack[0].borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                if file_info_rc.borrow().file_info_ast.borrow().indexed_module.is_none() {
                    file_info_rc.borrow_mut().prepare_ast(session);
                }
                let file_info = file_info_rc.borrow();
                if file_info.file_info_ast.borrow().indexed_module.is_some() {
                    let file_info_ast = file_info.file_info_ast.borrow();
                    let func_index = self.sym_stack[0].borrow().node_index().unwrap().load();
                    if func_index != NodeIndex::NONE {
                        let stmt = file_info_ast.indexed_module.as_ref().unwrap().get_by_index(func_index);
                        let body = match stmt {
                            AnyRootNodeRef::Stmt(Stmt::FunctionDef(s)) => {
                                &s.body
                            },
                            _ => {panic!("Wrong statement in validation ast extraction {} ", sym_type)}
                        };
                        let old_noqa = session.current_noqa.clone();
                        session.current_noqa = self.sym_stack[0].borrow().get_noqas();
                        self.validate_body(session, body);
                        session.current_noqa = old_noqa;
                        match stmt {
                            AnyRootNodeRef::Stmt(Stmt::FunctionDef(_)) => {
                                self.sym_stack[0].borrow_mut().as_func_mut().diagnostics.insert(BuildSteps::VALIDATION, self.diagnostics.clone());
                            },
                            _ => {panic!("Wrong statement in validation ast extraction {} ", sym_type)}
                        }
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
                self.file_info.as_ref().unwrap().borrow_mut().publish_diagnostics(session);
            }
            if !session.sync_odoo.config.file_cache {
                if symbol.typ() == SymType::PACKAGE(PackageType::MODULE) {
                    let manifest_path = PathBuf::from(symbol.as_module_package().path.clone()).join("__manifest__.py").sanitize();
                    if let Some(manifest_file) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path) {
                        if !manifest_file.borrow().opened {
                            let manifest_file = manifest_file.borrow();
                            manifest_file.file_info_ast.borrow_mut().indexed_module = None;
                            manifest_file.file_info_ast.borrow_mut().text_rope = None;
                            manifest_file.file_info_ast.borrow_mut().text_hash = 0;
                        }
                    }
                }
                if let Some(file) = self.file_info.as_ref() {
                    if ! file.borrow().opened {
                        let f = file.borrow();
                        f.file_info_ast.borrow_mut().indexed_module = None;
                        f.file_info_ast.borrow_mut().text_rope = None;
                        f.file_info_ast.borrow_mut().text_hash = 0;
                    }
                }
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
            let old_noqa = session.current_noqa.clone();
            session.current_noqa = sym.borrow().get_noqas().clone();
            self.sym_stack.push(sym);
            self.validate_body(session, &c.body);
            self.sym_stack.pop();
            session.current_noqa = old_noqa;
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

    fn _resolve_import(&mut self, session: &mut SessionInfo, _from_stmt: Option<&Identifier>, name_aliases: &[Alias], _level: Option<u32>, _range: &TextRange) {
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
                                            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03003, &[&module.borrow().as_module_package().dir_name]) {
                                                self.diagnostics.push(Diagnostic {
                                                    range: Range::new(Position::new(alias.range.start().to_u32(), 0), Position::new(alias.range.end().to_u32(), 0)),
                                                    ..diagnostic_base.clone()
                                                });
                                            }
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
                let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None);
                for eval_weak in eval_weaks.iter() {
                    let Some(symbol) = eval_weak.upgrade_weak() else {continue};
                    if !symbol.borrow().is_field_class(session){
                        continue;
                    }
                    'related_check: {
                        if let Some(related_field_name) = eval_weak.as_weak().context.get(&S!("related")).filter(|val| matches!(val, ContextValue::STRING(_))).map(|ctx_val| ctx_val.as_string()) {
                            let Some(special_arg_range) = eval_weak.as_weak().context.get(&S!("related_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                break 'related_check;
                            };
                            let syms = PythonArchEval::get_nested_sub_field(session, &related_field_name, class.clone(), maybe_from_module.clone());
                            if syms.is_empty(){
                                if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03014, &[&related_field_name, &model_data.name]) {
                                    self.diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        ..diagnostic_base.clone()
                                    });
                                }
                                break 'related_check;
                            }
                            let Some(field_type) = symbol
                                .borrow()
                                .get_member_symbol(session, &S!("type"), None, false, false, false, false)
                                .0.first()
                                .and_then(|field_type_var| field_type_var.borrow().evaluations().cloned())
                                .and_then(|evals| evals.first().cloned())
                                .and_then(|eval| eval.value.clone())
                                .and_then(|value| match value {
                                    EvaluationValue::CONSTANT(Expr::StringLiteral(s)) => Some(s.value.to_string()),
                                    _ => None,
                                }) else {
                                break 'related_check;
                            };
                            let found_same_type_match = syms.iter().any(|sym|{
                                let related_eval_weaks = Symbol::follow_ref(&&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                                    Rc::downgrade(&sym),
                                    None,
                                    false,
                                )), session, &mut None, true, true, None);
                                related_eval_weaks.iter().any(|related_eval_weak|{
                                    let Some(related_field_class_sym) = related_eval_weak.upgrade_weak() else {
                                        return false
                                    };
                                    let found = related_field_class_sym
                                        .borrow()
                                        .get_member_symbol(session, &S!("type"), None, false, false, false, false)
                                        .0.first()
                                        .and_then(|field_type_var| field_type_var.borrow().evaluations().cloned())
                                        .and_then(|evals| evals.first().cloned())
                                        .and_then(|eval| eval.value.clone())
                                        .map(|value| matches!(value, EvaluationValue::CONSTANT(Expr::StringLiteral(s)) if s.value.to_string() == field_type))
                                        .unwrap_or(false);
                                    found
                                })
                            });
                            if !found_same_type_match{
                                if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03017, &[]) {
                                    self.diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        ..diagnostic_base.clone()
                                    });
                                }

                            }
                        }
                    }
                    'comodel_check: {
                        if let Some(comodel_field_name) = eval_weak.as_weak().context.get(&S!("comodel_name")).map(|ctx_val| ctx_val.as_string()) {
                            let Some(special_arg_range) = eval_weak.as_weak().context.get(&S!("comodel_name_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                break 'comodel_check;
                            };
                            let Some(file_symbol) = class_ref.get_file().and_then(|file| file.upgrade()) else {
                                break 'comodel_check;
                            };
                            let maybe_model = session.sync_odoo.models.get(&Sy!(comodel_field_name.clone()));
                            if maybe_model.map(|m| m.borrow_mut().has_symbols()).unwrap_or(false){
                                let model = maybe_model.unwrap().clone();
                                file_symbol.borrow_mut().add_model_dependencies(&model);
                                let Some(ref from_module) = maybe_from_module else {break 'comodel_check;};
                                if !model.clone().borrow().model_in_deps(session, from_module) {
                                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03015, &[&comodel_field_name]) {
                                        self.diagnostics.push(Diagnostic {
                                            range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                            ..diagnostic_base.clone()
                                        });
                                    }
                                } else {
                                    break 'comodel_check;
                                }
                            } else {
                                if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03016, &[&comodel_field_name]) {
                                    self.diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        ..diagnostic_base.clone()
                                    });
                                }
                            }
                            file_symbol.borrow_mut().as_file_mut().not_found_models.insert(Sy!(comodel_field_name.clone()), BuildSteps::ARCH_EVAL);
                            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol.clone());
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
                            true,
                            false
                        );
                        let method_found = !symbols.is_empty();
                        if !method_found{
                            let Some(arg_range) = eval_weak.as_weak().context.get(&format!("{special_fn_field_name}_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03018, &[&method_name]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }

                        }
                    }
                    if let Some(inverse_name) = eval_weak.as_weak().context.get(&S!("inverse_name")).map(|ctx_val| ctx_val.as_string()) {
                        let Some(model_name) = eval_weak.as_weak().context.get(&S!("comodel_name")).map(|ctx_val| ctx_val.as_string()) else {
                            continue;
                        };
                        let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", model_name)).cloned() else {
                            continue;
                        };
                        let Some(module) = class_ref.find_module() else {
                            continue;
                        };
                        let main_syms = model.borrow().get_main_symbols(session, Some(module.clone()));
                        let symbols: Vec<_> = main_syms.iter().flat_map(|main_sym|
                            main_sym.clone().borrow().get_member_symbol(session, &inverse_name, Some(module.clone()), false, true, false, true, false).0
                        ).collect();
                        let method_found = !symbols.is_empty();
                        if !method_found{
                            let Some(arg_range) = eval_weak.as_weak().context.get(&format!("inverse_name_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03021, &[&inverse_name, &model_name]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }
                        }
                        if symbols.iter().any(|sym| !sym.borrow().is_specific_field(session, &["Many2one"])) {
                            let Some(arg_range) = eval_weak.as_weak().context.get(&format!("inverse_name_arg_range")).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03022, &[]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }

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
                            self._check_module_dependency(session, class, &s.value.to_string(), &s.range());
                        },
                        EvaluationValue::LIST(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    self._check_module_dependency(session, class, &s.value.to_string(), &s.range());
                                }
                            }
                        },
                        EvaluationValue::TUPLE(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    self._check_module_dependency(session, class, &s.value.to_string(), &s.range());
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
        // Check name for shadowing warning
        let model_name = model_data.name.clone();
        let Some(model) = session.sync_odoo.models.get(&model_name).cloned() else {
            return;
        };
        let inherited_model_names = class_ref.as_class_sym()._model.as_ref().unwrap().inherit.clone();
        if !inherited_model_names.contains(&model_name)
        && model.borrow().get_main_symbols(session, class_ref.find_module()).into_iter().filter(|main_sym| {
            !Rc::ptr_eq(main_sym, class)
        }).count() > 0 {
            // This a model with a name that already exists in models and in dependencies,
            // and it is not inherited, so it is basically shadowing the existing model.
            let _name = class_ref.get_symbol(&(vec![], vec![Sy!("_name")]), u32::MAX);
            if let Some(_name) = _name.last() {
                let mut range = _name.borrow().range().clone();
                // Try to get the string value range, otherwise stick to _name var range.
                if let Some(eval_range) = _name.borrow().evaluations().unwrap().iter().find_map(|e|
                    match e.follow_ref_and_get_value(session, &mut None, &mut self.diagnostics) {
                        Some(EvaluationValue::CONSTANT(Expr::StringLiteral(_))) => e.range,
                        _ => None,
                    }
                ) {
                    range = TextRange::new(range.start(), eval_range.end());
                }
                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS03020, &[&model_name]) {
                    self.diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&range),
                        ..diagnostic
                    });
                }
            }
        }
        // check inherits
        let inherits = class_ref.get_symbol(&(vec![], vec![Sy!("_inherits")]), u32::MAX);
        if let Some(inherits) = inherits.last() {
            let inherits = inherits.borrow();
            let inherits_evals = &inherits.evaluations().unwrap();
            for inherits_eval in inherits_evals.iter() {
                let inherits_value = inherits_eval.follow_ref_and_get_value(session, &mut None, &mut vec![]);
                if let Some(inherits_value) = inherits_value {
                    match inherits_value {
                        EvaluationValue::DICT(d) => {
                            for (key, _value) in d.iter() {
                                if let Expr::StringLiteral(s) = key {
                                    self._check_module_dependency(session, class, &s.value.to_string(), &s.range());
                                }
                            }
                        },
                        _ => {warn!("wrong _inherits value");}
                    }
                }
            }
        }
    }

    fn _check_module_dependency(&mut self, session: &mut SessionInfo, class_sym_rc: &Rc<RefCell<Symbol>>, model: &String, range: &TextRange) {
        let Some(from) = self.current_module.as_ref() else {
            return; //TODO do we want to raise something?
        };
        let model_name = Sy!(model.clone());
        let model = session.sync_odoo.models.get(&model_name);
        if model.map(|m| m.borrow_mut().has_symbols()).unwrap_or(false) {
            let model = model.unwrap().clone();
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
                        break;
                    }
                }
            }
            if !found_one {
                if !main_modules.is_empty() {
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03004, &[]) {
                        self.diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                } else {
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03005, &[]) {
                        self.diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                }
            }
        } else {
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03002, &[]) {
                self.diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                    ..diagnostic_base
                });
            }
            let Some(file_symbol) = class_sym_rc.borrow().get_file().and_then(|file| file.upgrade()) else {
              return;
            };
            file_symbol.borrow_mut().as_file_mut().not_found_models.insert(model_name.clone(), BuildSteps::ARCH_EVAL);
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol.clone());
        }
    }

    fn validate_expr(&mut self, session: &mut SessionInfo, expr: &Expr, max_infer: &TextSize) {
        let mut deps = vec![vec![], vec![], vec![]];
        let (_, diags) = Evaluation::eval_from_ast(session, expr, self.sym_stack.last().unwrap().clone(), max_infer, false, &mut deps);
        Symbol::insert_dependencies(&self.file, &mut deps, BuildSteps::VALIDATION);
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
