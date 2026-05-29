use crate::utils::{HashMap, HashSet};
use std::rc::Rc;
use std::cell::RefCell;
use std::{u32, vec};

use ruff_text_size::{Ranged, TextRange, TextSize};
use ruff_python_ast::{Alias, AnyRootNodeRef, Expr, ExprNamed, FStringPart, Identifier, NodeIndex, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtExpr, StmtFor, StmtFunctionDef, StmtIf, StmtReturn, StmtTry, StmtWhile, StmtWith};
use lsp_types::{Diagnostic, Position, Range};
use tracing::{debug, trace, warn};

use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::entry_point::EntryPointType;
use crate::core::symbols::{ModuleSymbol, SymbolMgr};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::{ClassKey, FunctionKey, ModuleKey, SourceFileKey, SymbolKey, VariableKey};
use crate::core::symbols::VariableSymbol;
use crate::{constants::*, oyarn, Sy};
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Evaluation;
use crate::core::python_utils;
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;
use crate::S;

use super::config::DiagMissingImportsMode;
use super::entry_point::EntryPoint;
use super::evaluation::{ContextValue, EvaluationSymbolPtr, EvaluationSymbolWeak};
use super::file_mgr::FileMgr;
use super::import_resolver::ImportResult;
use super::python_arch_eval_hooks::PythonArchEvalHooks;
use super::python_odoo_builder::PythonOdooBuilder;
use super::python_utils::{Assign, AssignTargetType};
use super::symbols::function_symbol::FunctionSymbol;

#[derive(Debug, Clone)]
pub struct PythonArchEval {
    entry_point: Rc<RefCell<EntryPoint>>,
    file: SourceFileKey,
    file_mode: bool,
    current_step: BuildSteps,
    sym_stack: Vec<SymbolKey>,
    diagnostics: Vec<Diagnostic>,
    safe_import: Vec<bool>,
}

impl PythonArchEval {
    pub fn new(symbol_table: &SymbolTable, entry_point: Rc<RefCell<EntryPoint>>, symbol: SymbolKey) -> Option<Self> {
        if matches!(symbol, SymbolKey::Namespace(_) | SymbolKey::Root(_) | SymbolKey::Compiled(_) | SymbolKey::Variable(_) | SymbolKey::Class(_)) {
            return None; // nothing to evaluate
        }
        if symbol_table.build_status(symbol,BuildSteps::ARCH) != BuildStatus::DONE || symbol_table.build_status(symbol, BuildSteps::ARCH_EVAL) != BuildStatus::PENDING {
            return None;
        }
        let file = symbol_table.get_file(symbol).unwrap();
        let file_mode = symbol == file;
        Some(PythonArchEval {
            entry_point,
            file,
            file_mode,
            current_step: if file_mode {BuildSteps::ARCH_EVAL} else {BuildSteps::VALIDATION},
            sym_stack: vec![symbol],
            diagnostics: Vec::new(),
            safe_import: vec![false],
        })
    }

    pub fn eval_arch(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack[0];
        if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !session.st().is_external(symbol)) {
            trace!("ARCH_EVAL  - PYTHON {} - {}", session.st().path(self.file), session.st().name(symbol));
        }
        session.st_mut().set_build_status(symbol, BuildSteps::ARCH_EVAL, BuildStatus::IN_PROGRESS);
        let path = session.st().file_path(self.file);
        let Some(file_info_rc) = session.sync_odoo.get_file_mgr().borrow().get_file_info(path).clone() else {
            warn!("File info not found for {}", path);
            return;
        };
        if file_info_rc.borrow().file_info_ast.borrow().indexed_module.is_none() {
            file_info_rc.borrow_mut().prepare_ast(session);
        }
        let file_info = (*file_info_rc).borrow();
        let file_info_ast = file_info.file_info_ast.clone();
        drop(file_info);
        if let SymbolKey::Module(m) = symbol  {
            ModuleSymbol::load_data(m, session);
        }
        if file_info_ast.borrow().indexed_module.is_some() {
            let old_noqa = session.current_noqa.clone();
            session.current_noqa = session.st().get_noqas(symbol);
            let file_info_ast_bw  = file_info_ast.borrow();
            let (ast, maybe_func_stmt) = match self.file_mode {
                true => {
                    if file_info_ast_bw.text_hash != session.st().get_processed_text_hash(self.file) {
                        session.st_mut().set_build_status(symbol, BuildSteps::ARCH_EVAL, BuildStatus::INVALID);
                        return;
                    }
                    (file_info_ast_bw.get_stmts().unwrap(), None)
                },
                false => {
                    let f = self.sym_stack[0].unwrap_function_key();
                    let fun_index = session.st()[f].node_index.load();
                    if fun_index == NodeIndex::NONE{ // uninitialized node index
                        // Function has no body or is dynamically created from a hook
                        (&vec![], None) // essentially skip evaluation
                    } else {
                        let func_stmt = file_info_ast_bw.indexed_module.as_ref().unwrap().get_by_index(fun_index);
                        match func_stmt {
                            AnyRootNodeRef::Stmt(Stmt::FunctionDef(func_stmt)) => {
                                (&func_stmt.body, Some(func_stmt))
                            },
                            _ => panic!("Expected function definition")
                        }
                    }
                }
            };
            self.visit_sub_stmts(session, &ast);
            if !self.file_mode && let Some(func_stmt) = maybe_func_stmt {
                let f = self.sym_stack[0].unwrap_function_key();
                self.diagnostics.extend(
                    PythonArchEvalHooks::handle_func_decorators(session, func_stmt, f, self.file, self.current_step)
                );
                PythonArchEval::handle_function_returns(session, func_stmt, f, &ast.last().unwrap().range().end(), &mut self.diagnostics);
                PythonArchEval::handle_func_evaluations(&mut session.sync_odoo.symbol_table, &ast, f);
            }
            session.current_noqa = old_noqa;
        }
        if self.file_mode {
            file_info_rc.borrow_mut().replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
            PythonArchEvalHooks::on_file_eval(session, &self.entry_point, self.file);
        } else {
            //then Symbol must be a function
            let f = symbol.unwrap_function_key();
            session.st_mut()[f].replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
            PythonArchEvalHooks::on_function_eval(session, &self.entry_point, f);
        }
        session.st_mut().set_build_status(self.sym_stack[0], BuildSteps::ARCH_EVAL, BuildStatus::DONE);
        if session.st().is_external(self.sym_stack[0]) && (!self.file_mode  || !file_info_rc.borrow().opened) {
            if self.file_mode {
                FileMgr::delete_file_path(session, &session.st().file_path(self.file).to_string());
            }
        } else {
            if self.file_mode {
                session.sync_odoo.add_to_validations(self.sym_stack[0]);
            }
        }
    }

    fn visit_stmt(&mut self, session: &mut SessionInfo, stmt: &Stmt) {
        match stmt {
            Stmt::Import(import_stmt) => {
                self.eval_symbols_from_import_stmt(session, None, &import_stmt.names, 0, &import_stmt.range)
            },
            Stmt::ImportFrom(import_from_stmt) => {
                self.eval_symbols_from_import_stmt(session, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level, &import_from_stmt.range)
            },
            Stmt::ClassDef(class_stmt) => {
                self.visit_class_def(session, class_stmt);
            },
            Stmt::FunctionDef(func_stmt) => {
                self.visit_func_def(session, func_stmt);
            },
            Stmt::AnnAssign(ann_assign_stmt) => {
                self._visit_ann_assign(session, ann_assign_stmt);
            },
            Stmt::Assign(assign_stmt) => {
                self._visit_assign(session, assign_stmt);
            },
            Stmt::If(if_stmt) => {
                self._visit_if(session, if_stmt);
            },
            Stmt::Try(try_stmt) => {
                self._visit_try(session, try_stmt);
            },
            Stmt::For(for_stmt) => {
                self._visit_for(session, for_stmt);
            },
            Stmt::With(with_stmt) => {
                self.visit_with(session, with_stmt);
            },
            Stmt::Return(return_stmt) => {
                self._visit_return(session, return_stmt);
            },
            Stmt::Match(match_stmt) => {
                self._visit_match(session, match_stmt);
            },
            Stmt::While(while_stmt) => {
                self.visit_while(session, while_stmt);
            },
            Stmt::Expr(stmt_expression) => {
                self.visit_expr(session, &*stmt_expression.value);
            },
            Stmt::Assert(assert_stmt) => {
                self.visit_expr(session, &assert_stmt.test);
            }
            Stmt::AugAssign(aug_assign_stmt) => {
                self.visit_expr(session, &aug_assign_stmt.target);
                self.visit_expr(session, &aug_assign_stmt.value);
            }
            Stmt::Delete(stmt_delete) => {
                stmt_delete.targets.iter().for_each(|del_target_expr| self.visit_expr(session, del_target_expr));
            },
            Stmt::TypeAlias(stmt_type_alias) => {
                self.visit_expr(session, &stmt_type_alias.value);
            },
            Stmt::Raise(stmt_raise) => {
                stmt_raise.exc.as_ref().map(|stmt_exc| self.visit_expr(session, &stmt_exc));
                stmt_raise.cause.as_ref().map(|stmt_cause| self.visit_expr(session, &stmt_cause));
            },
            Stmt::Global(_stmt_global) => {},
            Stmt::Nonlocal(_stmt_nonlocal) => {},
            Stmt::Break(_) => {},
            Stmt::Continue(_) => {},
            Stmt::Pass(_) => {},
            Stmt::IpyEscapeCommand(_) => {},
        }
    }

    fn visit_expr(&mut self, session: &mut SessionInfo, expr: &Expr){
        match expr {
            Expr::Named(named_expr) => {
                self.visit_named_expr(session, &named_expr);
            },
            Expr::BoolOp(bool_op_expr) => {
                for expr in bool_op_expr.values.iter() {
                    self.visit_expr(session, &expr);
                }
            },
            Expr::BinOp(bin_op_expr) => {
                self.visit_expr(session, &bin_op_expr.left);
                self.visit_expr(session, &bin_op_expr.right);
            },
            Expr::UnaryOp(unary_op_expr) => {
                self.visit_expr(session, &unary_op_expr.operand);
            },
            Expr::If(_todo_if_expr) => {
                // TODO:
                // This needs complex handling of sections
            },
            Expr::Dict(dict_expr) => {
                dict_expr.iter().for_each(
                    |dict_item| {
                        dict_item.key.as_ref().map(|dict_key_expr| self.visit_expr(session, dict_key_expr));
                        self.visit_expr(session, &dict_item.value);
                    }
                );
            },
            Expr::Set(expr_set) => {
                expr_set.iter().for_each(
                    |set_el_expr| {
                        self.visit_expr(session, set_el_expr);
                    }
                );
            },
            Expr::ListComp(expr_list_comp) => {
                self.visit_expr(session, &expr_list_comp.elt);
            },
            Expr::SetComp(expr_set_comp) => {
                self.visit_expr(session, &expr_set_comp.elt);
            },
            Expr::DictComp(expr_dict_comp) => {
                self.visit_expr(session, &expr_dict_comp.key);
                self.visit_expr(session, &expr_dict_comp.value);
            },
            Expr::Await(expr_await) => {
                self.visit_expr(session, &expr_await.value);
            },
            Expr::Yield(expr_yield) => {
                expr_yield.value.as_ref().map(|yield_value| self.visit_expr(session, &yield_value));
            },
            Expr::YieldFrom(expr_yield_from) => {
                self.visit_expr(session, &expr_yield_from.value);
            },
            Expr::Compare(expr_compare) => {
                expr_compare.comparators.iter().for_each(|comp_expr| self.visit_expr(session, comp_expr));
            },
            Expr::Call(expr_call) => {
                self.visit_expr(session, &expr_call.func);
                expr_call.arguments.args.iter().for_each(|arg_expr| self.visit_expr(session, arg_expr));
                expr_call.arguments.keywords.iter().for_each(|keyword| self.visit_expr(session, &keyword.value));
            },
            Expr::FString(expr_fstring) => {
                expr_fstring.value.iter().for_each(|fstring_part|{
                    match fstring_part{
                        FStringPart::FString(fstr) => fstr.elements.interpolations().map(|interpolation| &interpolation.expression).for_each(
                            |expression| self.visit_expr(session, expression)
                        ),
                        FStringPart::Literal(_) => {},
                    }
                });
            },
            Expr::TString(expr_tstring) => {
                expr_tstring.value.iter().for_each(|tstring_part|{
                    tstring_part.elements.interpolations().map(|interpolation| &interpolation.expression).for_each(
                        |expression| self.visit_expr(session, expression)
                    );
                });
            },
            Expr::Subscript(expr_subscript) => {
                self.visit_expr(session, &expr_subscript.value);
                self.visit_expr(session, &expr_subscript.slice);
            },
            Expr::List(expr_list) => {
                expr_list.elts.iter().for_each(|elt_expr| self.visit_expr(session, elt_expr));
            },
            Expr::Tuple(expr_tuple) => {
                expr_tuple.elts.iter().for_each(|elt_expr| self.visit_expr(session, elt_expr));
            },
            Expr::Slice(expr_slice) => {
                expr_slice.upper.as_ref().map(|upper_expr| self.visit_expr(session, &upper_expr));
                expr_slice.lower.as_ref().map(|lower_expr| self.visit_expr(session, &lower_expr));
            },
            // Expressions that cannot contained a named expressions are not traversed
            Expr::Lambda(lambda_expr) => {
                let Some(lambda_sym) = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), "<lambda>", &lambda_expr.range) else {
                    return; // can be not found if AST is incomplete
                };
                let function_key = lambda_sym.unwrap_function_key();
                session.st_mut()[function_key].arch_eval_status = BuildStatus::IN_PROGRESS;
                self.sym_stack.push(lambda_sym);
                self.visit_expr(session, &lambda_expr.body);
                let mut deps = vec![vec![], vec![]];
                let (eval, diags) = Evaluation::eval_from_ast(session, &lambda_expr.body, lambda_sym, &lambda_expr.body.range().start(), false, &mut deps);
                self.diagnostics.extend(diags);
                session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
                FunctionSymbol::add_return_evaluations(function_key, session, eval);
                self.sym_stack.pop();
                session.st_mut()[function_key].arch_eval_status = BuildStatus::DONE;
            },
            Expr::Generator(_todo_expr_generator) => {
                // generators are lazily evaluated,
                // thus named expression are only invoked when the generator is iterated
                // which modifies the variable in it in a custom scope
                // No method to handle that now, and it is a very niche use that is safe to not handle
            },
            Expr::StringLiteral(_expr_string_literal) => {},
            Expr::BytesLiteral(_expr_bytes_literal) => {},
            Expr::NumberLiteral(_expr_number_literal) => {},
            Expr::BooleanLiteral(_expr_boolean_literal) => {},
            Expr::NoneLiteral(_expr_none_literal) => {},
            Expr::EllipsisLiteral(_expr_ellipsis_literal) => {},
            Expr::Attribute(_expr_attribute) => {},
            Expr::Starred(_expr_starred) => {},
            Expr::IpyEscapeCommand(_expr_ipy_escape_command) => {},
            Expr::Name(_expr_name) => {},
        }
    }

    fn _match_diag_config(&self, odoo: &mut SyncOdoo, symbol: SymbolKey) -> bool {
        let import_diag_level = &odoo.config.diag_missing_imports;
        if *import_diag_level == DiagMissingImportsMode::None {
            return false
        }
        if *import_diag_level == DiagMissingImportsMode::All {
            return true
        }
        if *import_diag_level == DiagMissingImportsMode::OnlyOdoo {
            let tree = odoo.symbol_table.get_tree(symbol);
            if tree.0.len() > 0 && tree.0[0] == "odoo" {
                return true;
            }
        }
        false
    }

    /// Follow the evaluations of sym_ref and return true if the end evaluation contains from_sym
    fn check_for_cyclic_evaluation(&mut self, session: &mut SessionInfo, sym_ref: SymbolKey, from_sym: VariableKey) -> bool {
        let syms_followed = SymbolTable::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
            sym_ref, None, false
        )), session, &mut None, false, false, None, None);
        for sym in syms_followed {
            let Some(sym) = sym.upgrade_weak(session.st()) else { continue };
            if sym == from_sym {
                return true;
            }
        }
        false
    }

    fn eval_symbols_from_import_stmt(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: u32, _range: &TextRange) {
        if name_aliases.len() == 1 && name_aliases[0].name.to_string() == "*" {
            return;
        }
        let import_results: Vec<ImportResult> = resolve_import_stmt(
            session,
            self.file.into(),
            from_stmt,
            name_aliases,
            level,
            &mut Some(&mut self.diagnostics));

        for import_result in import_results.iter() {
            let variable = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &import_result.var_name, &import_result.range);
            let Some(SymbolKey::Variable(variable_key)) = variable else {
                continue;
            };
            if import_result.found {
                session.st_mut()[variable_key].evaluations = vec![];
                for &import_sym in import_result.symbols.iter() {
                    let has_loop = self.check_for_cyclic_evaluation(session, import_sym, variable_key);
                    if !has_loop { //anti-loop. We want to be sure we are not evaluating to the same sym
                        let instance = match import_sym {
                            SymbolKey::Class(_) => Some(false),
                            _ => None
                        };
                        let evaluation = Evaluation::eval_from_symbol(session.st(), import_sym, instance);
                        session.st_mut()[variable_key].evaluations.push(evaluation);
                        let file_of_import_symbol = session.st().get_file(import_sym);
                        if let Some(import_file) = file_of_import_symbol {
                            if self.file != import_file {
                                session.st_mut().add_dependency(self.file, import_file, self.current_step, BuildSteps::ARCH);
                            }
                        }
                    } else {
                        let mut file_tree = import_result.file_tree.clone();
                        file_tree.extend(import_result.name.split(".").map(|s| oyarn!("{}", s)));
                        session.st_mut().not_found_paths_mut(self.file).push((self.current_step, file_tree.clone()));
                        self.entry_point.borrow_mut().not_found_symbols.insert(self.file);
                        if self._match_diag_config(session.sync_odoo, import_sym) {
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS02002, &[&file_tree.clone().join(".")]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(import_result.range.start().to_u32(), 0), Position::new(import_result.range.end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                        }
                    }
                }

            } else {
                let mut file_tree = import_result.file_tree.clone();
                file_tree.extend(import_result.name.split(".").map(|s| oyarn!("{}", s)));
                if session.sync_odoo.config.diag_missing_imports != DiagMissingImportsMode::All && BUILT_IN_LIBS.contains(&file_tree[0].as_str()) {
                    continue;
                }
                if !self.safe_import.last().unwrap() {
                    session.st_mut().not_found_paths_mut(self.file).push((self.current_step, file_tree.clone()));
                    self.entry_point.borrow_mut().not_found_symbols.insert(self.file);
                    for &import_sym in import_result.symbols.iter() {
                        if self._match_diag_config(session.sync_odoo, import_sym) {
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS02001, &[&file_tree.clone().join(".")]) {
                                self.diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(import_result.range.start().to_u32(), 0), Position::new(import_result.range.end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_assigns(&mut self, session: &mut SessionInfo, assigns: Vec<Assign>, range: &TextRange){
        for assign in assigns.iter() {
            if let Some(ref expr) = assign.value {
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    let variable = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &name_expr.id, &name_expr.range);
                    if let Some(SymbolKey::Variable(variable_key)) = variable {
                        let parent = session.st()[variable_key].parent();
                        if assign.annotation.is_none() && assign.value.is_none() {
                            panic!("either value or annotation should exists");
                        }
                        let mut deps = vec![vec![], vec![]];
                        if !self.file_mode {
                            deps.push(vec![]);
                        }
                        let mut ann_evaluations = assign.annotation.as_ref().map(|annotation| Evaluation::eval_from_ast(session, annotation, parent, &range.start(), true, &mut deps));
                        session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
                        deps = vec![vec![], vec![]];
                        if !self.file_mode {
                            deps.push(vec![]);
                        }
                        let value_evaluations = assign.value.as_ref().map(|value| Evaluation::eval_from_ast(session, value, parent, &range.start(), false, &mut deps));
                        session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
                        let mut take_value = false;
                        if let Some((ref val_eval, ref _diags)) = value_evaluations {
                            if val_eval.len() == 1 {
                                let evaluation = &val_eval[0];
                                let sym_weak = evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], Some(parent));
                                if let Some(sym_key) = sym_weak.weak.upgrade(session.st()) {
                                    if SymbolTable::is_field_class(session, sym_key) {
                                        take_value = true;
                                    }
                                }
                            }
                            if !take_value{
                                take_value = ann_evaluations.is_none();
                            }
                        }
                        let (eval, diags) = if take_value {
                            value_evaluations.unwrap()
                        } else {
                            if value_evaluations.is_some() {
                                ann_evaluations.as_mut().unwrap().0.extend(value_evaluations.unwrap().0);
                            }
                            ann_evaluations.unwrap()
                        };
                        let v_mut = &mut session.st_mut()[variable_key];
                        v_mut.evaluations.extend(eval);
                        self.diagnostics.extend(diags);
                        let var_name = v_mut.name.clone();
                        let evaluations = v_mut.evaluations.clone();
                        let mut dep_to_add = vec![];
                        let mut to_remove = vec![];
                        for (ix, evaluation) in evaluations.iter().enumerate() {
                            if let Some(sym) = evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, None).weak.upgrade(session.st()) {
                                if sym == variable_key {
                                    // TODO: investigate deps, and fix cyclic evals
                                    let file_path = session.st().get_file(parent).map(|file| session.st().path(file));
                                    warn!("Found cyclic evaluation symbol: {}, parent: {}, file: {}", var_name, session.st().name(parent), file_path.unwrap_or("N/A"));
                                    to_remove.push(ix);
                                    continue;
                                }
                                if let Some(file) = session.st().get_file(sym) {
                                    if self.file != file {
                                        dep_to_add.push(file);
                                    }
                                }
                            }
                        }
                        let v_mut = &mut session.st_mut()[variable_key];
                        for ix in to_remove.into_iter().rev() {
                            v_mut.evaluations.remove(ix);
                        }
                        for dep in dep_to_add {
                            session.st_mut().add_dependency(self.file, dep, self.current_step, BuildSteps::ARCH);
                        }
                    } else {
                        debug!("Symbol not found");
                    }
                },
                AssignTargetType::Attribute(ref attr_expr) => {
                    // Validation for compute methods, only in function mode
                    if self.file_mode {
                        continue;
                    }
                    // Checks if we are in a class method, and if the attribute is a field of the model
                    let Some(parent_class) = session.st().get_in_parents(self.sym_stack[0], &[SymType::CLASS], true) else {
                        continue;
                    };

                    // let parent_class = parent_class.borrow();
                    let c = parent_class.unwrap_class_key();
                    let Some(model_data) = session.st()[c]._model.as_ref() else {
                        continue;
                    };
                    let Some(model) = session.sync_odoo.models.get(&model_data.name).cloned() else {
                        continue;
                    };
                    let model_classes = model.borrow().all_symbols(session, session.st().find_module(parent_class), false);
                    let fn_name = session.st().name(self.sym_stack[0]).clone();
                    let allowed_fields: HashSet<_> = model_classes.iter().filter_map(|(sym, _)|
                        session.st()[*sym]._model.as_ref().unwrap().computes.get(&fn_name).cloned()
                    ).flatten().collect();
                    if allowed_fields.is_empty() {
                        continue;
                    }

                    let mut expr = Expr::Attribute(attr_expr.clone());
                    let mut invalid_field = false;
                    let mut valid_field = false;
                    // Check the  whole attribute chain, to see if we are in a field of the model that is valid
                    // so for z.a.b.c, checks, z.a, z.a.b, z.a.b.c, if one of them is valid it is okay
                    'while_block: while matches!(expr, Expr::Attribute(_)){
                        let assignee = Evaluation::eval_from_ast(session, &expr, *self.sym_stack.last().unwrap(), &attr_expr.range.start(), false, &mut vec![]);
                        for evaluation in assignee.0 {
                            let evaluation_symbol_ptr = evaluation.symbol.get_symbol_weak_transformed(session, &mut None, &mut vec![], None);
                            let Some(sym_key) = evaluation_symbol_ptr.upgrade_weak(session.st()) else {
                                continue;
                            };
                            if !SymbolTable::is_field(session, sym_key) {
                                continue;
                            }
                            let field_name = session.st().name(sym_key).clone();
                            if allowed_fields.contains(&field_name){
                                valid_field = true;
                                break 'while_block;
                            }
                            invalid_field = true;
                        }
                        expr = *expr.as_attribute_expr().unwrap().value.clone();
                    }

                    // If there is some modified fields in the method, that are not the correct ones, show diagnostic
                    if !valid_field && invalid_field {
                        if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS03019, &[]) {
                            self.diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(attr_expr.range.start().to_u32(), 0), Position::new(attr_expr.range.end().to_u32(), 0)),
                                ..diagnostic
                            });
                        }
                    }
                }
            }
        }
    }

    fn  _visit_ann_assign(&mut self, session: &mut SessionInfo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&[*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&[*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        self.handle_assigns(session, assigns, &ann_assign_stmt.range);
    }

    fn _visit_assign(&mut self, session: &mut SessionInfo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        self.handle_assigns(session, assigns, &assign_stmt.range);
    }

    fn visit_named_expr(&mut self, session: &mut SessionInfo, named_expr: &ExprNamed) {
        let assigns = python_utils::unpack_assign(&[*named_expr.target.clone()], None, Some(&named_expr.value));
        self.handle_assigns(session, assigns, &named_expr.range);
    }

    fn load_base_classes(&mut self, session: &mut SessionInfo, loc_sym: ClassKey, class_stmt: &StmtClassDef) {
        for base in class_stmt.bases() {
            let mut deps = vec![vec![], vec![]];
            let eval_base = Evaluation::eval_from_ast(session, base, *self.sym_stack.last().unwrap(), &class_stmt.range().start(), false, &mut deps);
            session.st_mut().insert_dependencies(self.file, &deps, BuildSteps::ARCH_EVAL);
            self.diagnostics.extend(eval_base.1);
            let eval_base = eval_base.0;
            if eval_base.len() == 0 {
                //TODO build tree for not_found_path
                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS01001, &[&AstUtils::flatten_expr(base)]) {
                    self.diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(base.range().start().to_u32(), 0), Position::new(base.range().end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
                continue;
            }
            if eval_base.len() > 1 {
                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS01003, &[&AstUtils::flatten_expr(base)]) {
                    self.diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(base.range().start().to_u32(), 0), Position::new(base.range().end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
                continue;
            }
            let eval_base = &eval_base[0];
            let eval_symbol = eval_base.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let ref_sym = SymbolTable::follow_ref(&eval_symbol, session, &mut None, false, true, None, None);
            if ref_sym.len() > 1 {
                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS01003, &[&AstUtils::flatten_expr(base)]) {
                    self.diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(base.range().start().to_u32(), 0), Position::new(base.range().end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
                continue;
            }
            let symbol = ref_sym[0].upgrade_weak(session.st());
            let Some(symbol) = symbol else {
                continue;
            };
            if matches!(symbol, SymbolKey::Compiled(_)) {
                continue; //Compiled classes do not have their bases loaded
            }
            if let SymbolKey::Class(c) = symbol {
                //Even if this is a valid class, we have to be sure that its own bases should have been loaded already
                let sym_file = session.st().get_file(symbol);
                if let Some(file) = sym_file {
                    if session.st().build_status(file.into(), BuildSteps::ARCH_EVAL) != BuildStatus::DONE {
                        SyncOdoo::build_now(session, file, BuildSteps::ARCH_EVAL);
                    }
                    if self.file != file {
                        session.st_mut().add_dependency(self.file, file, self.current_step, BuildSteps::ARCH_EVAL);
                    }
                }
                session.st_mut()[loc_sym].bases.push(c.into());
            } else if !matches!(symbol, SymbolKey::Variable(_)) || session.st()[symbol.unwrap_variable_key()].is_value() { // if it's a variable and not a value, it means we can't evaluate it, let's skip diagnostic
                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS01002, &[&AstUtils::flatten_expr(base)]) {
                    self.diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(base.start().to_u32(), 0), Position::new(base.end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
            }
        }
    }

    fn visit_sub_stmts(&mut self, session: &mut SessionInfo, stmts: &Vec<Stmt>){
        stmts.iter().for_each(|stmt| self.visit_stmt(session, stmt));
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, class_stmt: &StmtClassDef) {
        let Some(class_key) = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &class_stmt.name, &class_stmt.range) else {
            return;
        };
        let c = class_key.unwrap_class_key();
        self.load_base_classes(session, c, class_stmt);
        let old_noqa = session.current_noqa.clone();
        session.current_noqa = session.st().get_noqas(class_key);
        self.sym_stack.push(class_key);
        self.visit_sub_stmts(session, &class_stmt.body);
        self.sym_stack.pop();
        if !session.st().is_external(self.sym_stack[0]) && session.st().get_entry(self.sym_stack[0]).borrow().typ == EntryPointType::MAIN {
            if session.st().get_in_parents(class_key, &[SymType::FUNCTION], true).is_some() {
                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS03024, &[]) {
                    self.diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&class_stmt.name.range),
                        ..diagnostic
                    });
                }
            } else {
                let odoo_builder_diags = PythonOdooBuilder::new(c).load(session);
                self.diagnostics.extend(odoo_builder_diags);
            }
        }
        session.current_noqa = old_noqa;
    }

    fn visit_func_def(&mut self, session: &mut SessionInfo, func_stmt: &StmtFunctionDef) {
        let scope = *self.sym_stack.last().unwrap();
        let function_sym = session.st().get_positioned_symbol(scope, &func_stmt.name, &func_stmt.range);
        let Some(function_sym_key) = function_sym else {
            return; // can be not found if AST is incomplete
        };
        let f = function_sym_key.unwrap_function_key();
        let func = &session.st()[f];
        let is_static = func.is_static;
        if func.can_be_in_class() || !matches!(scope, SymbolKey::Class(_)) {
            let mut is_first = true;
            for arg in func_stmt.parameters.posonlyargs.iter().chain(&func_stmt.parameters.args) {
                if is_first && !is_static && matches!(scope, SymbolKey::Class(_)) {
                    let is_class_method = session.st()[f].is_class_method;
                    let arg_name = OYarn::from(arg.parameter.name.id.to_string());
                    let arg_sym = session.st()[f].symbols().get(&arg_name).unwrap().get(&0).unwrap()[0]; //get first declaration
                    let v = arg_sym.unwrap_variable_key();
                    let evaluation = Evaluation::new_self(scope, Some(!is_class_method));
                    session.st_mut()[v].evaluations.push(evaluation);
                    is_first = false;
                    continue;
                }
                is_first = false;
                if arg.parameter.annotation.is_some() {
                    let mut deps = vec![vec![], vec![]];
                    if !self.file_mode {
                        deps.push(vec![]);
                    }
                    let (eval, diags) = Evaluation::eval_from_ast(session,
                                                &arg.parameter.annotation.as_ref().unwrap(),
                                                scope,
                                                &func_stmt.range.start(),
                                                true,
                                                &mut deps);
                    session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
                    let arg_name = OYarn::from(arg.parameter.name.id.to_string());
                    let arg_sym = session.st()[f].symbols().get(&arg_name).unwrap().get(&0).unwrap()[0];
                    let v = arg_sym.unwrap_variable_key();
                    session.st_mut()[v].evaluations = eval;
                    self.diagnostics.extend(diags);
                } else if arg.default.is_some() {
                    let mut deps = vec![vec![], vec![]];
                    if !self.file_mode {
                        deps.push(vec![]);
                    }
                    let (eval, diags) = Evaluation::eval_from_ast(session,
                                                arg.default.as_ref().unwrap(),
                                                scope,
                                                &func_stmt.range.start(),
                                                false,
                                                &mut deps);
                    session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
                    let arg_name = OYarn::from(arg.parameter.name.id.to_string());
                    let arg_sym = session.st()[f].symbols().get(&arg_name).unwrap().get(&0).unwrap()[0];
                    let v = arg_sym.unwrap_variable_key();
                    session.st_mut()[v].evaluations = eval;
                    self.diagnostics.extend(diags);
                }
            }
        } else if !is_static {
            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS01004, &[]) {
                self.diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&func_stmt.name.range()),
                    ..diagnostic
                });
            }
        }
        if !self.file_mode || session.st().get_in_parents(function_sym_key, &[SymType::CLASS], true).is_none() {
            session.st_mut()[f].arch_eval_status = BuildStatus::IN_PROGRESS;
            let old_noqa = session.current_noqa.clone();
            session.current_noqa = session.st().get_noqas(function_sym_key);
            self.sym_stack.push(function_sym_key);
            self.visit_sub_stmts(session, &func_stmt.body);
            self.sym_stack.pop();
            session.current_noqa = old_noqa;
            PythonArchEval::handle_function_returns(session, func_stmt, f, &func_stmt.range.end(), &mut self.diagnostics);
            PythonArchEval::handle_func_evaluations(&mut session.sync_odoo.symbol_table, &func_stmt.body, f);
            session.st_mut()[f].arch_eval_status = BuildStatus::DONE;
        }
    }

    fn _visit_if(&mut self, session: &mut SessionInfo, if_stmt: &StmtIf) {
        self.visit_expr(session, &if_stmt.test);
        self.visit_sub_stmts(session, &if_stmt.body);
        if_stmt.elif_else_clauses.iter().for_each(|elif_clause| {
            elif_clause.test.as_ref().map(|test_clause| self.visit_expr(session, &test_clause));
            self.visit_sub_stmts(session, &elif_clause.body)
        });
    }

    fn _visit_for(&mut self, session: &mut SessionInfo, for_stmt: &StmtFor) {
        self.visit_expr(session, &for_stmt.iter);
        let mut deps = vec![vec![], vec![]];
        if !self.file_mode {
            deps.push(vec![]);
        }
        let (eval_iter_node, diags) = Evaluation::eval_from_ast(session,
            &for_stmt.iter,
            *self.sym_stack.last().unwrap(),
            &for_stmt.target.range().start(), false, &mut deps);
        session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
        self.diagnostics.extend(diags);
        if eval_iter_node.len() == 1 { //Only handle values that we are sure about
            let eval = &eval_iter_node[0];
            let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            if !eval_symbol.is_expired_if_weak(session.st()) {
                let symbol_eval = SymbolTable::follow_ref(&eval_symbol, session, &mut None, false, false, None, None);
                if symbol_eval.len() == 1 && let Some(symbol_type) = symbol_eval[0].upgrade_weak(session.st()) {
                    if matches!(symbol_type, SymbolKey::Class(_)) {
                        let (iters, _) = SymbolTable::get_member_symbol(session, symbol_type, &S!("__iter__"), None, true, false, false, false, false);
                        if let Some(&SymbolKey::Function(iter)) = iters.first() && iters.len() == 1 {
                            SyncOdoo::ensure_func_evaluations(session, iter);
                            let evals = &session.st()[iter].evaluations;
                            if evals.len() == 1 {
                                let eval_iter = evals[0].clone();
                                if for_stmt.target.is_name_expr() { //only handle simple variable for now
                                    let variable = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &for_stmt.target.as_name_expr().unwrap().id, &for_stmt.target.range());
                                    let symbol = eval_iter.symbol.get_symbol_as_weak(session, &mut Some(HashMap::from_iter([(S!("parent_for"), ContextValue::SYMBOL(symbol_type.into()))])), &mut vec![], None);
                                    let v = variable.unwrap().unwrap_variable_key();
                                    session.st_mut()[v].evaluations = vec![Evaluation::eval_from_symbol(session.st(), symbol.weak, symbol.instance)];
                                }
                            }
                        }
                    }
                }
            }
        }
        self.visit_sub_stmts(session, &for_stmt.body);
        //TODO split evaluation
        self.visit_sub_stmts(session, &for_stmt.orelse);
    }

    fn _visit_try(&mut self, session: &mut SessionInfo, try_stmt: &StmtTry) {
        let mut safe_import = false;
        for handler in try_stmt.handlers.iter() {
            let handler = handler.as_except_handler().unwrap();
            if let Some(type_) = &handler.type_ {
                if type_.is_name_expr() && type_.as_name_expr().unwrap().id.to_string() == "ImportError" {
                    safe_import = true;
                }
            }
        }
        self.safe_import.push(safe_import);
        self.visit_sub_stmts(session, &try_stmt.body);
        self.safe_import.pop();
        self.visit_sub_stmts(session, &try_stmt.orelse);
        self.visit_sub_stmts(session, &try_stmt.finalbody);
        for handler in try_stmt.handlers.iter() {
            handler.as_except_handler().map(|h| {
                //Prevent import error in catch clause of ImportError too
                let mut added_safe_import = false;
                if let Some(type_) = &h.type_ {
                    if type_.is_name_expr() && type_.as_name_expr().unwrap().id.to_string() == "ImportError" {
                        added_safe_import = true;
                        self.safe_import.push(true);
                    }
                }
                h.type_.as_ref().map(|test_clause| self.visit_expr(session, test_clause));
                self.visit_sub_stmts(session, &h.body);
                if added_safe_import {
                    self.safe_import.pop();
                }
            });
        }
    }

    fn _visit_return(&mut self, session: &mut SessionInfo, return_stmt: &StmtReturn) {
        if let Some(value) = return_stmt.value.as_ref() {
            self.visit_expr(session, &value);
        }
        let func = self.sym_stack.last().unwrap().clone();
        if let SymbolKey::Function(f) = func {
            if let Some(value) = return_stmt.value.as_ref() {
                let mut deps = vec![vec![], vec![]];
                if !self.file_mode {
                    deps.push(vec![]);
                }
                let (eval, diags) = Evaluation::eval_from_ast(session, value, func, &return_stmt.range.start(), false, &mut deps);
                session.sync_odoo.symbol_table.insert_dependencies(self.file, &deps, self.current_step);
                self.diagnostics.extend(diags);
                FunctionSymbol::add_return_evaluations(f, session, eval);
            } else {
                FunctionSymbol::add_return_evaluations(f, session, vec![Evaluation::new_none()]);
            }
        }
    }

    fn visit_with(&mut self, session: &mut SessionInfo, with_stmt: &StmtWith) {
        for item in with_stmt.items.iter() {
            self.visit_expr(session, &item.context_expr);
            if let Some(var) = item.optional_vars.as_ref() {
                match var.as_ref() {
                    Expr::Name(expr_name) => {
                        let variable = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &expr_name.id, &expr_name.range());
                        if let Some(SymbolKey::Variable(variable_key)) = variable {
                            let parent = session.st()[variable_key].parent();
                            let mut deps = vec![vec![], vec![]];
                            if !self.file_mode {
                                deps.push(vec![]);
                            }
                            let (context_mgr_evals, diags) = Evaluation::eval_from_ast(session, &item.context_expr, parent, &with_stmt.range.start(), false, &mut deps);
                            session.st_mut().insert_dependencies(self.file, &deps, self.current_step);
                            // The expression name in with <> [as <name>], is the result of __enter__.
                            let mut enter_evals = vec![];
                            for context_mgr_eval in context_mgr_evals.iter() {
                                let symbol = context_mgr_eval.symbol.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, Some(session.st().parent_file_or_function(variable_key.into()).unwrap()));
                                if let Some(symbol) = symbol.weak.upgrade(session.st()) {
                                    let _enter_ = session.st().get_symbol(symbol, &(vec![], vec![Sy!("__enter__")]), u32::MAX);
                                    if let Some(&SymbolKey::Function(_enter_)) = _enter_.last() {
                                        SyncOdoo::ensure_func_evaluations(session, _enter_);
                                        enter_evals.extend(session.st()[_enter_].evaluations.clone());
                                    }
                                }
                            }
                            session.st_mut()[variable_key].evaluations = enter_evals;
                            self.diagnostics.extend(diags);
                        }
                    },
                    Expr::Tuple(_) => {continue;},
                    Expr::List(_) => {continue;},
                    _ => {continue;}
                }
            }

        }
        self.visit_sub_stmts(session, &with_stmt.body);
    }

    fn _visit_match(&mut self, session: &mut SessionInfo<'_>, match_stmt: &ruff_python_ast::StmtMatch) {
        match_stmt.cases.iter().for_each(|case| {
            case.guard.as_ref().map(|test_clause| self.visit_expr(session, test_clause));
            self.visit_sub_stmts(session, &case.body)
        });
    }

    fn visit_while(&mut self, session: &mut SessionInfo, while_stmt: &StmtWhile) {
        self.visit_expr(session, &while_stmt.test);
        self.visit_sub_stmts(session, &while_stmt.body);
        self.visit_sub_stmts(session, &while_stmt.orelse);
    }

    // Handle function return annotation
    // Evaluate return annotation and add it to function evaluations
    fn handle_function_returns(
        session: &mut SessionInfo,
        func_stmt: &StmtFunctionDef,
        func_sym: FunctionKey,
        max_infer: &TextSize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {

        if let Some(returns_ann) = func_stmt.returns.as_ref() {
            let mut deps = vec![vec![], vec![]];
            let parent = session.st()[func_sym].parent();
            let (mut evaluations, diags) = Evaluation::eval_from_ast(
                session,
                &returns_ann,
                parent,
                max_infer,
                true,
                &mut deps,
            );
            // Check for type annotation `typing.Self`, if so, return a `self` evaluation
            // And give it priority over other evaluations
            if evaluations.iter().any(|evaluation|
                SymbolTable::follow_ref(
                    &evaluation.symbol.get_symbol(session, &mut None, diagnostics, None),
                    session,
                    &mut None,
                    false,
                    false,
                    Some((vec![Sy!("typing")], vec![Sy!("Self")])),
                    None
                ).len() > 0
            ){
                if let Some(base) = session.st().get_in_parents(func_sym.into(), &[SymType::CLASS], true) {
                    let is_class_method = session.st()[func_sym].is_class_method;
                    session.st_mut()[func_sym].evaluations = vec![Evaluation::new_self(base, Some(!is_class_method))];
                }
                return;
            }
            for eval in evaluations.iter_mut() { //as this is an evaluation, we need to set the instance to true
                match eval.symbol.get_mut_symbol_ptr() {
                    EvaluationSymbolPtr::WEAK(sym_weak) => {
                        sym_weak.instance = Some(true);
                    },
                    _ => {}
                }
            }
            if let Some(file_sym) = session.st().get_file(func_sym.into()) {
                session.st_mut().insert_dependencies(file_sym, &mut deps, BuildSteps::ARCH_EVAL);
            }
            diagnostics.extend(diags);
            session.st_mut()[func_sym].evaluations = evaluations;
        }
    }

    // Handle function evaluation if traversing the body did not get any evaluations
    // First we check if it is a function signature with no body ( like in stubs ) like def func():...
    // If so we give it an Any evaluation because it is undetermined, otherwise we give it None, because that means
    // we have a body but no return statement, which defaults to return None at the end
    fn handle_func_evaluations(
        symbol_table: &mut SymbolTable,
        func_body: &Vec<Stmt>,
        func_sym: FunctionKey,
    ){
        let func_mut = &mut symbol_table[func_sym];
        if func_mut.evaluations.is_empty() {
            let has_implementation = !matches!(
                func_body.first(),
                Some(Stmt::Expr(StmtExpr { range: _, value:  x, node_index: _})) if matches!(**x, Expr::EllipsisLiteral(_))
            );
            func_mut.evaluations  = vec![
                if has_implementation {
                    Evaluation::new_none()
                } else {
                    Evaluation::new_any()
                }
            ];
        }
    }

    pub fn get_nested_sub_field(
        session: &mut SessionInfo,
        field_name: &str,
        class_sym: ClassKey,
        from_module: Option<ModuleKey>,
    ) -> Vec<SymbolKey>{
        let mut parent_object = Some(class_sym);
        let mut syms = vec![];
        let split_expr: Vec<String> = field_name.split(".").map(|x| x.to_string()).collect();
        for (ix, name) in split_expr.iter().enumerate() {
            if parent_object.is_none() {
                break;
            }
            let (symbols, _diagnostics) = SymbolTable::get_member_symbol(session,
                parent_object.unwrap().into(),
                name,
                from_module,
                false,
                true,
                false,
                true,
                false);
            if ix == split_expr.len() - 1 {
                syms = symbols;
                break;
            } else if symbols.is_empty() {
                break;
            }
            parent_object = None;
            for s in symbols {
                if !SymbolTable::is_specific_field(session, s, &["Many2one", "One2many", "Many2many"]) {
                    break;
                }
                let models = VariableSymbol::get_relational_model(s.unwrap_variable_key(), session, from_module);
                if models.len() == 1 {
                    parent_object = Some(models[0]);
                    break;
                }
            }
        }
        syms
    }
}
