use ruff_python_ast::{Alias, AnyRootNodeRef, Expr, Identifier, NodeIndex, Stmt, StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign, StmtClassDef, StmtMatch, StmtRaise, StmtTry, StmtTypeAlias, StmtWith};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::{trace, warn};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::Path;
use lsp_types::{Diagnostic, Position, Range};
use crate::core::build_scheduler::BuildScheduler;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::evaluation_context::{ContextKey, ContextValue};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::storage::xml::xml_field_symbol::XmlFieldName;
use crate::core::symbols::symbol_keys::{ClassKey, ModelSymbolKey, ModuleKey, PythonBuildableSymbolKey, SourceFileKey, SymbolKey};
use crate::{constants::*, oyarn};
use crate::core::symbols::{ModuleSymbol};
use crate::threads::SessionInfo;
use crate::utils::{PathSanitizer as _};
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak, EvaluationValue};
use super::file_mgr::{FileInfo, FileMgr};
use super::import_resolver::manual_import;
use super::python_arch_eval::PythonArchEval;

#[derive(Debug)]
pub struct PythonValidator {
    entry_point: Rc<RefCell<EntryPoint>>,
    file: SourceFileKey,
    file_mode: bool,
    sym_stack: Vec<SymbolKey>,
    pub diagnostics: Vec<Diagnostic>, //collect diagnostic from arch and arch_eval too from inner functions, but put everything at Validation level
    safe_imports: Vec<bool>,
    current_module: Option<ModuleKey>,
    file_info: Option<Rc<RefCell<FileInfo>>>,
}

/* PythonValidator operate on a single Symbol. Unlike other steps, it can be done on symbol containing code (file and functions only. Not class, variable, namespace).
It will validate this node and run a validator on all subsymbol and dependencies.
It will try to inference the return type of functions if it is not annotated; */
impl PythonValidator {
    pub fn new(symbol_table: &SymbolTable, entry_point: Rc<RefCell<EntryPoint>>, symbol: PythonBuildableSymbolKey) -> Self {
        Self {
            entry_point,
            file: symbol_table.get_file(symbol.into()).unwrap(),
            file_mode: true,
            sym_stack: vec![symbol.into()],
            diagnostics: vec![],
            safe_imports: vec![false],
            current_module: symbol_table.find_module(symbol),
            file_info: None,
        }
    }

    /* Validate the symbol. The dependencies must be done before any validation. */
    pub fn validate(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack[0];
        if !session.st().ready_for_step(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION) {
            return;
        }
        let (file_info_rc, loaded) = FileMgr::get_or_recreate_file_info(session, self.file);
        if !loaded {
            session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION, BuildStatus::INVALID);
            return;
        }
        self.file_info = Some(file_info_rc.clone());
        match symbol {
            SymbolKey::File(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) => {
                let source_file_key = symbol.as_source_file_key().unwrap();
                if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !session.st().is_external(symbol)) {
                    trace!("VALIDATION - PYTHON FILE {}", session.st().paths(symbol).first().unwrap_or(&S!("No path found")));
                }
                session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                file_info_rc.borrow_mut().replace_diagnostics(DiagnosticSource::PY_VALIDATION, vec![]);
                if !file_info_rc.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info_rc.borrow_mut().prepare_ast(session);
                }
                let file_info = file_info_rc.borrow();
                if file_info_rc.borrow().file_info_ast.borrow().text_hash != session.st().get_processed_text_hash(source_file_key) {
                    session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION, BuildStatus::INVALID);
                    return;
                }
                let file_info_ast_rc = file_info.file_info_ast.clone();
                let file_info_ast = file_info_ast_rc.borrow();
                drop(file_info);
                if file_info_ast.ast.as_py_ast().indexed_module.is_some() {
                    let old_noqa = session.current_noqa.clone();
                    session.current_noqa = session.st().get_noqas(symbol);
                    self.validate_body(session, file_info_ast.get_stmts().as_ref().unwrap());
                    session.current_noqa = old_noqa;
                }
                drop(file_info_ast);
                let symbol = self.sym_stack[0];
                if let SymbolKey::Module(m) = symbol {
                    ModuleSymbol::validate_manifest(m, session);
                }
                let mut file_info = file_info_rc.borrow_mut();
                file_info.replace_diagnostics(DiagnosticSource::PY_VALIDATION, self.diagnostics.clone());
            },
            SymbolKey::Function(f) => {
                if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !session.st().is_external(symbol)) {
                    trace!("VALIDATION - PYTHON FUNCTION: {}", session.st().name(symbol));
                }
                self.file_mode = false;
                let Some(parent_file) = session.st().get_file(symbol) else {
                    panic!("Parent file not found on validating function")
                };
                if file_info_rc.borrow().file_info_ast.borrow().text_hash != session.st_mut().get_processed_text_hash(parent_file) {
                    session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION, BuildStatus::INVALID);
                    return;
                }
                if !session.st().ready_for_step(f.into(), BuildSteps::VALIDATION) {
                    return;
                }
                self.diagnostics = vec![];
                session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                if !file_info_rc.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info_rc.borrow_mut().prepare_ast(session);
                }
                let file_info = file_info_rc.borrow();
                let file_info_ast_rc = file_info.file_info_ast.clone();
                let file_info_ast = file_info_ast_rc.borrow();
                drop(file_info);
                if let Some(indexed_module) = &file_info_ast.ast.as_py_ast().indexed_module {
                    let func_index = session.st()[f].node_index.load();
                    if func_index != NodeIndex::NONE {
                        let stmt = indexed_module.get_by_index(func_index);
                        let body = match stmt {
                            AnyRootNodeRef::Stmt(Stmt::FunctionDef(s)) => {
                                &s.body
                            },
                            _ => {panic!("Wrong statement in validation ast extraction {} ", SymType::FUNCTION)}
                        };
                        let old_noqa = session.current_noqa.clone();
                        session.current_noqa = session.st().get_noqas(symbol);
                        self.validate_body(session, body);
                        session.current_noqa = old_noqa;
                        match stmt {
                            AnyRootNodeRef::Stmt(Stmt::FunctionDef(_)) => {
                                let f = self.sym_stack[0].unwrap_function_key();
                                session.st_mut()[f].diagnostics.insert(BuildSteps::VALIDATION, self.diagnostics.clone());
                            },
                            _ => {panic!("Wrong statement in validation ast extraction {} ", SymType::FUNCTION)}
                        }
                    }
                } else {
                    warn!("no ast found on file info");
                }
            },
            _ => {panic!("Only File, function can be validated")}
        }
        let symbol = self.sym_stack[0];
        session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::VALIDATION, BuildStatus::DONE);
        if matches!(symbol.typ(), SymType::FILE | SymType::PACKAGE(_)) {
            if !session.st().in_workspace(symbol) {
                if !session.st().is_external(symbol) {
                    return;
                }
                let file_info = self.file_info.as_ref().unwrap();
                if !file_info.borrow().opened { // Never delete opened files
                    let file = symbol.as_source_file_key().unwrap();
                    let file_path = session.sync_odoo.symbol_table.file_path(file).to_string();
                    FileMgr::delete_file_path(session, &file_path);
                }
            } else {
                self.file_info.as_ref().unwrap().borrow_mut().publish_diagnostics(session);
            }
            if !session.sync_odoo.config.file_cache() {
                if let SymbolKey::Module(m) = symbol {
                    let manifest_path = Path::new(&session.st()[m].path).join("__manifest__.py");
                    if let Some(manifest_file) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize_cow())
                        && !manifest_file.borrow().opened {
                            let manifest_file = manifest_file.borrow();
                            manifest_file.file_info_ast.borrow_mut().ast.as_py_ast_mut().indexed_module = None;
                            manifest_file.file_info_ast.borrow_mut().text_document = None;
                            manifest_file.file_info_ast.borrow_mut().text_hash = 0;
                        }
                }
                if let Some(file) = self.file_info.as_ref()
                    && ! file.borrow().opened {
                        let f = file.borrow();
                        f.file_info_ast.borrow_mut().ast.as_py_ast_mut().indexed_module = None;
                        f.file_info_ast.borrow_mut().text_document = None;
                        f.file_info_ast.borrow_mut().text_hash = 0;
                    }
            }
        }
    }

    fn validate_body(&mut self, session: &mut SessionInfo, vec_ast: &[Stmt]) {
        for stmt in vec_ast.iter() {
            match stmt {
                Stmt::FunctionDef(f) => {
                    let sym = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &f.name, &f.range);
                    if let Some(sym) = sym {
                        if session.st().ready_for_step(sym.unwrap_buildable_key(), BuildSteps::ARCH) {
                            BuildScheduler::build_now(session, sym.unwrap_buildable_key(), BuildSteps::ARCH);
                        }
                        if session.st().ready_for_step(sym.unwrap_buildable_key(), BuildSteps::ARCH_EVAL) {
                            BuildScheduler::build_now(session, sym.unwrap_buildable_key(), BuildSteps::ARCH_EVAL);
                        }
                        if session.st().ready_for_step(sym.unwrap_buildable_key(), BuildSteps::VALIDATION) {
                            if let Some(python_buildable) = sym.as_python_buildable() {
                                let mut v = PythonValidator::new(session.st(), self.entry_point.clone(), python_buildable);
                                v.validate(session);
                            }
                        } else if session.st().build_status(sym.unwrap_buildable_key(), BuildSteps::VALIDATION) == BuildStatus::IN_PROGRESS {
                            panic!("cyclic validation detected... Aborting");
                        }
                        let f = sym.unwrap_function_key();
                        self.diagnostics.extend(session.st()[f].diagnostics.values().flat_map(|v| v.clone()));
                    }
                },
                Stmt::ClassDef(c) => {
                    self.visit_class_def(session, c);
                },
                Stmt::Try(t) => {
                    self.visit_try(session, t);
                },
                Stmt::Import(i) => {
                    self.resolve_import(session, None, &i.names, None, &i.range);
                },
                Stmt::ImportFrom(i) => {
                    self.resolve_import(session, i.module.as_ref(), &i.names, Some(i.level), &i.range);
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
        let sym = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &c.name, &c.range);
        if let Some(sym) = sym {
            self._check_model(session, sym.unwrap_class_key());
            let old_noqa = session.current_noqa.clone();
            session.current_noqa = session.st().get_noqas(sym).clone();
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
            if let Some(type_) = &handler.type_
                && type_.is_name_expr() && type_.as_name_expr().unwrap().id == "ImportError" {
                    safe_import = true;
                }
        }
        self.safe_imports.push(safe_import);
        self.validate_body(session, &node.body);
        self.safe_imports.pop();
    }

    fn resolve_import(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], _level: Option<u32>, _range: &TextRange) {
        let file_symbol = session.st().get_file(self.sym_stack[0]).expect("file symbol not found");
        for alias in name_aliases.iter() {
            if alias.name.id == "*" {
                continue;
            }
            let Some(current_module) = self.current_module else {
                continue;
            };
            let var_name = match &alias.asname {
                Some(asname) => asname,
                None => alias.name.split(".").next().unwrap(),
            };
            let variable = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), var_name, &alias.range);
            if let Some(variable) = variable {
                let v = variable.unwrap_variable_key();
                for evaluation in session.st()[v].evaluations.clone() {
                    let eval_sym = evaluation.symbol.get_symbol(session, None, &mut self.diagnostics, Some(file_symbol.into()));
                    match eval_sym {
                        EvaluationSymbolPtr::WEAK(w) => {
                            if let Some(symbol) = w.weak.upgrade(session.st()) {
                                self.check_import_module_in_deps(session, current_module, symbol, &alias.range);
                            }
                        },
                        _ => {
                            panic!("Internal error: evaluated has invalid evaluationType");
                        }
                    }
                }
            }
            // Check normal deep import statements, e.g. from odoo.addons.<module>
            if from_stmt.is_none() && alias.asname.is_none() && alias.name.contains('.') {
                let deep_results = manual_import(session, file_symbol.into(), None, alias.name.as_str(), Some(S!("_")), 0, &mut None);
                for result in deep_results {
                    for symbol in result.symbols {
                        self.check_import_module_in_deps(session, current_module, symbol, &alias.range);
                    }
                }
            }
        }
    }

    fn check_import_module_in_deps(&mut self, session: &mut SessionInfo, current_module: ModuleKey, symbol: SymbolKey, range: &TextRange) {
        let Some(module) = session.st().find_module(symbol) else {
            return;
        };
        let dir_name = session.st()[module].dir_name.clone();
        if !ModuleSymbol::is_in_deps(session.st(), current_module, &dir_name) && !self.safe_imports.last().unwrap()
            && let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03003, &[&dir_name]) {
                self.diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                    ..diagnostic_base
                });
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

    fn _check_model(&mut self, session: &mut SessionInfo, class: ClassKey) {
        let Some(model_data) = session.st()[class]._model.as_ref() else {
            return;
        };
        let model_name = model_data.name.clone();
        if self.current_module.is_none() {
            return;
        }
        let maybe_from_module = session.st().find_module(class);
        // Check fields, check related and comodel arguments
        for symbol in session.st().all_symbols(class.into()) {
            let SymbolKey::Variable(v) = symbol else {
                continue;
            };
            let evals = session.st()[v].evaluations.clone();
            for eval in evals.iter() {
                let symbol = eval.symbol.get_symbol(session, None,  &mut vec![], None);
                let eval_weaks = SymbolTable::follow_ref(&symbol, session, None, true, false, None, None);
                for eval_weak in eval_weaks {
                    let Some(symbol) = eval_weak.upgrade_weak(session.st()) else {continue};
                    if !SymbolTable::is_field_class(session, symbol) {
                        continue;
                    }
                    'related_check: {
                        if let Some(related_field_name) = eval_weak.get_weak().context.get(ContextKey::Related).filter(|val| matches!(val, ContextValue::STRING(_))).map(ContextValue::as_str) {
                            let Some(special_arg_range) = eval_weak.get_weak().context.get(ContextKey::RelatedArgRange).map(|ctx_val| ctx_val.as_text_range()) else {
                                break 'related_check;
                            };
                            let syms = PythonArchEval::get_nested_sub_field(session, related_field_name, class, maybe_from_module);
                            if syms.is_empty() {
                                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03014, &[related_field_name, &model_name]) {
                                    self.diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        ..diagnostic_base.clone()
                                    });
                                }
                                break 'related_check;
                            }
                            let Some(field_type) = SymbolTable::get_member_symbol(session, symbol, "type", None, false, false, false, false, false) .0.first()
                                .and_then(|field_type_var| session.st().evaluations(*field_type_var))
                                .and_then(|evals| evals.first().cloned())
                                .and_then(|eval| eval.value.clone())
                                .and_then(|value| value.as_string_literal().map(|s| s.value.to_string())) else {
                                break 'related_check;
                            };
                            let found_same_type_match = syms.iter().any(|&sym| {
                                let related_eval_weaks = SymbolTable::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                                    sym,
                                    None,
                                    false,
                                )), session, None, true, true, None, None);
                                related_eval_weaks.iter().any(|related_eval_weak|{
                                    let Some(related_field_class_sym) = related_eval_weak.upgrade_weak(session.st()) else {
                                        return false
                                    };

                                    SymbolTable::get_member_symbol(session, related_field_class_sym, "type", None, false, false, false, false, false)
                                        .0.first()
                                        .and_then(|field_type_var| session.st().evaluations(*field_type_var))
                                        .and_then(|evals| evals.first().cloned())
                                        .and_then(|eval| eval.value.clone())
                                        .map(|value| value.as_string_literal().is_some_and(|s| s.value.to_str() == field_type))
                                        .unwrap_or(false)
                                })
                            });
                            if !found_same_type_match
                                && let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03017, &[]) {
                                    self.diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        ..diagnostic_base.clone()
                                    });
                                }
                        }
                    }
                    'comodel_check: {
                        if let Some(comodel_field_name) = eval_weak.get_weak().context.get(ContextKey::ComodelName).map(ContextValue::as_str) {
                            let Some(special_arg_range) = eval_weak.get_weak().context.get(ContextKey::ComodelNameArgRange).map(|ctx_val| ctx_val.as_text_range()) else {
                                break 'comodel_check;
                            };
                            let Some(file_symbol) = session.st().get_file(class.into()) else {
                                break 'comodel_check;
                            };
                            let maybe_model = session.sync_odoo.models.get(comodel_field_name);
                            if maybe_model.map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false) {
                                let model = maybe_model.unwrap().clone();
                                session.st_mut().add_model_dependencies(file_symbol, &model);
                                let Some(from_module) = maybe_from_module else {break 'comodel_check;};
                                if !model.clone().borrow().model_in_deps(session, from_module) {
                                    if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03015, &[comodel_field_name]) {
                                        self.diagnostics.push(Diagnostic {
                                            range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                            ..diagnostic_base.clone()
                                        });
                                    }
                                } else {
                                    break 'comodel_check;
                                }
                            } else {
                                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03016, &[comodel_field_name]) {
                                    self.diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(special_arg_range.start().to_u32(), 0), Position::new(special_arg_range.end().to_u32(), 0)),
                                        ..diagnostic_base.clone()
                                    });
                                }
                            }
                            let file_key = file_symbol.unwrap_file_key();
                            session.st_mut()[file_key].not_found_models.insert(oyarn!("{comodel_field_name}"), BuildSteps::ARCH_EVAL);
                            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol);
                        }
                    }
                    for (special_fn_field_name, special_fn_field_arg_range) in [
                        (ContextKey::Compute, ContextKey::ComputeArgRange),
                        (ContextKey::Inverse, ContextKey::InverseArgRange),
                        (ContextKey::Search, ContextKey::SearchArgRange),
                    ]{
                        let Some(method_name) = eval_weak.get_weak().context.get(special_fn_field_name).map(ContextValue::as_str) else {
                            continue;
                        };
                        let Some(module) = maybe_from_module else {
                            continue;
                        };
                        let (symbols, _diagnostics) = SymbolTable::get_member_symbol(session, class.into(),
                            method_name,
                            Some(module),
                            false,
                            false,
                            true,
                            true,
                            false
                        );
                        let method_found = !symbols.is_empty();
                        if !method_found{
                            let Some(arg_range) = eval_weak.get_weak().context.get(special_fn_field_arg_range).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03018, &[method_name]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }

                        }
                    }
                    if let Some(inverse_name) = eval_weak.get_weak().context.get(ContextKey::InverseName).map(ContextValue::as_str) {
                        let Some(comodel_name) = eval_weak.get_weak().context.get(ContextKey::ComodelName).map(ContextValue::as_str) else {
                            continue;
                        };
                        let Some(model) = session.sync_odoo.models.get(comodel_name).cloned() else {
                            continue;
                        };
                        let Some(module) = maybe_from_module else {
                            continue;
                        };
                        let main_syms = model
                            .borrow()
                            .get_main_symbols(session, Some(module))
                            .filter_map(|k| k.as_class_key())
                            .collect::<Vec<_>>();
                        let symbols: Vec<_> = main_syms.iter().flat_map(|&main_sym|
                            SymbolTable::get_member_symbol(session, main_sym.into(), inverse_name, Some(module), false, true, false, true, false).0
                        ).collect();
                        let valid_xml_field_found = {
                            // Look for XML field that has name, ttype, and relation set
                            // Whose name matches the inverse name, is many2one and has the relation set to the current model
                            model
                                .borrow()
                                .get_xml_model_field_symbols(session.st(), Some(module))
                                .any(|rec_key| {
                                    let name = session.st()[rec_key]
                                        .get_field_text(XmlFieldName::Name, session.st())
                                        .unwrap_or_default();
                                    if name != inverse_name {
                                        return false;
                                    }
                                    let ttype = session.st()[rec_key]
                                        .get_field_text(XmlFieldName::Type, session.st())
                                        .unwrap_or_default();
                                    if !["many2one", "many2one_reference"].contains(&ttype.as_str())
                                    {
                                        return false;
                                    }
                                    let relation = session.st()[rec_key]
                                        .get_field_text(XmlFieldName::Relation, session.st())
                                        .unwrap_or_default();
                                    relation == comodel_name
                                })
                        };
                        if valid_xml_field_found {
                            continue;
                        }
                        if symbols.is_empty() {
                            let Some(arg_range) = eval_weak.get_weak().context.get(ContextKey::InverseNameArgRange).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03021, &[inverse_name, comodel_name]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }
                        }
                        if symbols.iter().any(|&sym| !SymbolTable::is_specific_field(session, sym, &["Many2one", "Many2oneReference"])) {
                            let Some(arg_range) = eval_weak.get_weak().context.get(ContextKey::InverseNameArgRange).map(|ctx_val| ctx_val.as_text_range()) else {
                                continue;
                            };
                            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03022, &[]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(arg_range.start().to_u32(), 0), Position::new(arg_range.end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }
                        } else {
                            // Check if we have a many2one field pointing to the comodel with another name than the current model
                            let mut comodel_eval_weaks = Vec::new();
                            for sym in symbols {
                                let evals = session.st().evaluations(sym).cloned().unwrap();
                                for eval in evals {
                                    let followed = SymbolTable::follow_ref(
                                        &eval.symbol.get_symbol(session, None, &mut vec![], None),
                                        session,
                                        None,
                                        true,
                                        false,
                                        None,
                                        None,
                                    );
                                    comodel_eval_weaks.extend(followed);
                                }
                            }
                            // The inverse field may be declared several times, e.g. when a
                            // model overrides the field of an inherited abstract model to
                            // point to the concrete model instead. As the last declaration
                            // overrides the comodel of the previous ones at runtime, the
                            // inverse field is valid as soon as one declaration points to
                            // the current model.
                            let mut mismatched_model_name = None;
                            for comodel_eval_weak in comodel_eval_weaks {
                                let Some(comodel_name) = comodel_eval_weak.get_weak().context.get(ContextKey::ComodelName).map(ContextValue::as_str) else {
                                    continue;
                                };
                                if model_name == comodel_name { // valid
                                    mismatched_model_name = None;
                                    break;
                                }
                                mismatched_model_name.get_or_insert(comodel_name.to_string());
                            }
                            if let Some(comodel_name) = mismatched_model_name {
                                let Some(arg_range) = eval_weak.get_weak().context.get(ContextKey::InverseNameArgRange).map(|ctx_val| ctx_val.as_text_range()) else {
                                    continue;
                                };
                                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03023, &[inverse_name, &model_name, &comodel_name]) {
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
        }
        //Check inherit field
        let inherit = session.st().get_symbol(class.into(), (&[], &["_inherit"]), u32::MAX);
        if let Some(&inherit) = inherit.last() {
            let inherit_evals = session.st().evaluations(inherit).cloned().unwrap();
            for inherit_eval in inherit_evals {
                let inherit_value = inherit_eval.follow_ref_and_get_value(session, None, &mut vec![]);
                if let Some(inherit_value) = inherit_value {
                    match inherit_value {
                        EvaluationValue::CONSTANT(c) => {
                            if let Expr::StringLiteral(s) = c.as_ref() {
                                self._check_module_dependency(session, class, s.value.to_str(), &s.range());
                            }
                        },
                        EvaluationValue::LIST(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    self._check_module_dependency(session, class, s.value.to_str(), &s.range());
                                }
                            }
                        },
                        EvaluationValue::TUPLE(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    self._check_module_dependency(session, class, s.value.to_str(), &s.range());
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
        let Some(model) = session.sync_odoo.models.get(&model_name).cloned() else {
            return;
        };
        let inherited_model_names = session.st()[class]._model.as_ref().unwrap().inherit.clone();
        if !inherited_model_names.contains(&model_name)
            // Defining it here because it is an expensive call
            && let conflicting_symbols = model
                .borrow()
                .get_main_symbols(session, maybe_from_module)
                .filter(|&main_sym| main_sym != class.into())
                .collect::<Vec<_>>()
            && !conflicting_symbols.is_empty()
        {
            // This a model with a name that already exists in models and in dependencies,
            // and it is not inherited, so it is basically shadowing the existing model.
            let _name = session.st().get_symbol(class.into(), (&[], &["_name"]), u32::MAX);
            if let Some(&_name) = _name.last() {
                let mut range = session.st().range(_name);
                let evals = session.st().evaluations(_name).cloned().unwrap();
                // Try to get the string value range, otherwise stick to _name var range.
                if let Some(eval_range) = evals.iter().find_map(|e|
                    match e.follow_ref_and_get_value(session, None, &mut self.diagnostics) {
                        Some(v) if v.as_string_literal().is_some() => e.range,
                        _ => None,
                    }
                ) {
                    range = TextRange::new(range.start(), eval_range.end());
                }
                let has_xml_model_conflict = conflicting_symbols.iter().any(|key| matches!(key, ModelSymbolKey::XmlRecord(_)));
                let diagnostic_code = if has_xml_model_conflict {
                    DiagnosticCode::OLS03303
                } else {
                    DiagnosticCode::OLS03020
                };
                if let Some(diagnostic) = create_diagnostic(session, diagnostic_code, &[&model_name]) {
                    self.diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&range),
                        ..diagnostic
                    });
                }
            }
        }
        // check inherits
        let inherits = session.st().get_symbol(class.into(), (&[], &["_inherits"]), u32::MAX);
        if let Some(&inherits) = inherits.last() {
            let inherits_evals = session.st().evaluations(inherits).cloned().unwrap();
            for inherits_eval in inherits_evals {
                let inherits_value = inherits_eval.follow_ref_and_get_value(session, None, &mut vec![]);
                if let Some(inherits_value) = inherits_value {
                    match inherits_value {
                        EvaluationValue::DICT(d) => {
                            for (key, _value) in d.iter() {
                                if let Expr::StringLiteral(s) = key {
                                    self._check_module_dependency(session, class, s.value.to_str(), &s.range());
                                }
                            }
                        },
                        _ => {warn!("wrong _inherits value");}
                    }
                }
            }
        }
    }

    fn _check_module_dependency(&mut self, session: &mut SessionInfo, class_key: ClassKey, model_name: &str, range: &TextRange) {
        let Some(from) = self.current_module else {
            return; //TODO do we want to raise something?
        };
        let model = session.sync_odoo.models.get(model_name);
        if model.map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false) {
            let model = model.unwrap().clone();
            let borrowed_model = model.borrow();
            let mut main_modules = vec![];
            let mut found_one = false;
            for main_sym in borrowed_model.get_main_symbols(session, None).filter_map(|sym| sym.as_class_key()) {
                let main_sym_module = session.st().find_module(main_sym);
                if let Some(main_sym_module) = main_sym_module {
                    let module_name = &session.st()[main_sym_module].dir_name;
                    main_modules.push(module_name.clone());
                    if ModuleSymbol::is_in_deps(session.st(), from, module_name) {
                        found_one = true;
                        break;
                    }
                }
            }
            if !found_one {
                if !main_modules.is_empty() {
                    if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03004, &[]) {
                        self.diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                } else {
                    if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03005, &[]) {
                        self.diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                }
            }
        } else {
            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03002, &[]) {
                self.diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                    ..diagnostic_base
                });
            }
            let Some(file_symbol) = session.st().get_file(class_key.into()) else {
              return;
            };
            let file_key = SymbolKey::from(file_symbol).unwrap_file_key();
            session.st_mut()[file_key].not_found_models.insert(oyarn!("{}", model_name), BuildSteps::ARCH_EVAL);
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol);
        }
    }

    fn validate_expr(&mut self, session: &mut SessionInfo, expr: &Expr, max_infer: &TextSize) {
        let mut deps = vec![vec![], vec![], vec![]];
        let (_, diags) = Evaluation::eval_from_ast(session, expr, *self.sym_stack.last().unwrap(), max_infer, false, &mut deps);
        session.sync_odoo.symbol_table.insert_dependencies(self.file, &deps, BuildSteps::VALIDATION);
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
