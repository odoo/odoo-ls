use lsp_types::Diagnostic;
use ruff_python_ast::{
    Alias, AnyRootNodeRef, BoolOp, CmpOp, Expr, ExprNamed, ExprTuple, FStringPart, Identifier, Parameters,
    Pattern, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFor, StmtFunctionDef, StmtIf,
    StmtMatch, StmtTry, StmtWhile, StmtWith,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::cell::RefCell;
use std::rc::Rc;
use std::vec;
use tracing::{trace, warn};

use crate::constants::{
    BuildStatus, BuildSteps, DEBUG_STEPS, DEBUG_STEPS_ONLY_INTERNAL, DiagnosticSource, OYarn, SymType
};
use crate::core::build_scheduler::BuildScheduler;
use crate::core::evaluation::{Evaluation, EvaluationValue};
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::python_arch_builder_hooks::PythonArchBuilderHooks;
use crate::core::python_utils;
use crate::core::type_narrowing::{match_isinstance_check, narrowing_range, IsinstanceCheck};
use crate::core::symbols::Buildable;
use crate::core::symbols::symbol_keys::{FunctionKey, PythonBuildableSymbolKey, SourceFileKey, SymbolKey, Wk};
use crate::core::symbols::storage::SymbolTable;
use crate::threads::SessionInfo;
use crate::{oyarn, S};

use super::entry_point::EntryPoint;
use super::evaluation::{EvaluationSymbolPtr, EvaluationSymbolWeak};
use super::file_mgr::{combine_noqa_info, FileInfo, FileMgr};
use super::import_resolver::ImportResult;
use super::odoo::SyncOdoo;
use super::python_utils::AssignTargetType;
use super::symbols::function_symbol::{Argument, ArgumentType};
use super::symbols::ModuleSymbol;
use super::symbols::symbol_mgr::SectionIndex;

#[derive(Debug)]
pub struct PythonArchBuilder {
    entry_point: Rc<RefCell<EntryPoint>>,
    file: SourceFileKey,
    file_mode: bool,
    current_step: BuildSteps,
    sym_stack: Vec<SymbolKey>,
    __all_symbols_to_add: Vec<(String, TextRange)>,
    diagnostics: Vec<Diagnostic>,
    file_info: Option<Rc<RefCell<FileInfo>>>,
}

impl PythonArchBuilder {
    pub fn new(symbol_table: &SymbolTable, entry_point: Rc<RefCell<EntryPoint>>, symbol: PythonBuildableSymbolKey) -> Option<Self> {
        let file = symbol_table.get_file(symbol.into()).unwrap();
        let file_mode = SymbolKey::from(symbol) == SymbolKey::from(file);

        Some(PythonArchBuilder {
            entry_point,
            file,
            file_mode,
            current_step: if file_mode {BuildSteps::ARCH} else {BuildSteps::VALIDATION},
            sym_stack: vec![symbol.into()],
            __all_symbols_to_add: Vec::new(),
            diagnostics: vec![],
            file_info: None,
        })
    }

    pub fn load_arch(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack[0];
        if !session.st().ready_for_step(symbol.unwrap_buildable_key(), BuildSteps::ARCH) {
            return;
        }
        if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !session.st().is_external(symbol)) {
            trace!("ARCH       - PYTHON {} - {}", session.st().path(self.file), session.st().name(symbol));
        }
        session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let path = session.st().file_path(self.file).to_string();
        if self.file_mode {
            let in_workspace = session.st().parent(self.file)
                .is_some_and(|parent| session.st().in_workspace(parent)) ||
                SyncOdoo::is_in_workspace_or_entry(session, &path);
            session.st_mut().set_in_workspace(self.file.into(), in_workspace);
        }
        if let SymbolKey::Module(m) = symbol  {
            let odoo_addons = session.st()[m].parent();
            ModuleSymbol::load_module_arch(m, session, odoo_addons);
        }
        let (file_info_rc, _) = FileMgr::get_or_recreate_file_info(session, self.file);
        if self.file_mode && !file_info_rc.borrow().file_info_ast.borrow().ast.is_built() {
            file_info_rc.borrow_mut().prepare_ast(session);
        }
        self.file_info = Some(file_info_rc.clone());
        if self.file_mode {
            //diagnostics for functions are stored directly on funcs
            let mut file_info = file_info_rc.borrow_mut();
            file_info.replace_diagnostics(DiagnosticSource::PY_ARCH, self.diagnostics.clone());
        }
        let file_info = file_info_rc.borrow();
        let file_info_ast_rc = file_info.file_info_ast.clone();
        let file_noqa =if self.file_mode {
             file_info.noqas_blocs.get(&0).cloned()
        } else {
            None
        };
        drop(file_info);
        let file_info_ast= file_info_ast_rc.borrow();
        if let Some(indexed_module) = &file_info_ast.ast.as_py_ast().indexed_module {
            let ast = if self.file_mode {
                file_info_ast.get_stmts().unwrap()
            } else {
                //  If the file has been re-parsed since, those indexes address another tree.
                if file_info_ast.text_hash != session.st().get_processed_text_hash(self.file) {
                    session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::ARCH, BuildStatus::INVALID);
                    return;
                }
                let f = self.sym_stack[0].unwrap_function_key();
                let ast_index = session.st()[f].node_index.load();
                if ast_index.as_u32().is_some() {
                    let func = indexed_module.get_by_index(ast_index);
                    match func {
                        AnyRootNodeRef::Stmt(Stmt::FunctionDef(func_stmt)) => {
                            func_stmt.body.as_slice()
                        },
                        _ => panic!("Expected function definition")
                    }
                } else {
                    //if ast_index is empty, this is because the function has been added manually and do not belong to the ast. Skip it's building
                    &[]
                }
            };
            let old_stack_noqa = session.noqas_stack.clone();
            session.noqas_stack.clear();
            let old_noqa = if self.file_mode {
                if let Some(file_noqa) = file_noqa {
                    session.noqas_stack.push(file_noqa);
                }
                let new_noqa = combine_noqa_info(&session.noqas_stack);
                session.st_mut().set_noqas(symbol, new_noqa.clone()); //only set for file, functions are set in visit_func_def
                let old = session.current_noqa.clone();
                session.current_noqa = new_noqa;
                session.st_mut().set_processed_text_hash(self.file, file_info_ast.text_hash);
                old
            } else {
                let noqas = session.st().get_noqas(symbol);
                session.noqas_stack.push(noqas.clone());
                let old = session.current_noqa.clone();
                session.current_noqa = noqas;
                old
            };
            self.visit_node(session, ast);
            session.current_noqa = old_noqa;
            session.noqas_stack = old_stack_noqa;
            self._resolve_all_symbols(session);
            session.st_mut().set_build_status(self.sym_stack[0].unwrap_buildable_key(), BuildSteps::ARCH, BuildStatus::DONE);
            if self.file_mode {
                BuildScheduler::queue(session, self.sym_stack[0].unwrap_buildable_key());
            }
        } else if self.file_mode {
            session.st_mut().set_build_status(self.sym_stack[0].unwrap_buildable_key(), BuildSteps::ARCH, BuildStatus::DONE);
            if matches!(symbol, SymbolKey::Module(_)) {
                //even if there is no __init__.py, we need to go to rebuild_arch and validation to validate the manifest
                BuildScheduler::queue(session, self.sym_stack[0].unwrap_buildable_key());
            } else {
                let mut file_info = file_info_rc.borrow_mut();
                file_info.publish_diagnostics(session);
            }
        } else {
            session.st_mut().set_build_status(self.sym_stack[0].unwrap_buildable_key(), BuildSteps::ARCH, BuildStatus::INVALID)
        }
        if self.file_mode {
            PythonArchBuilderHooks::on_file_done(session, self.file);
        }
    }

    fn create_local_symbols_from_import_stmt(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: u32, _range: &TextRange) {
        for import_name in name_aliases {
            if import_name.name.as_str() == "*" {
                if self.sym_stack.len() != 1 { //only at top level for now.
                    continue;
                }
                let import_result: ImportResult = resolve_import_stmt(
                    session,
                    *self.sym_stack.last().unwrap(),
                    from_stmt,
                    name_aliases,
                    level,
                    &mut None).remove(0); //we don't need the vector with this call as there will be 1 result.
                if !import_result.found {
                    self.entry_point.borrow_mut().not_found_symbols.insert(self.file);
                    session.st_mut().not_found_paths_mut(self.file).push((self.current_step, import_result.file_tree.clone()));
                    continue;
                }
                let mut all_name_allowed = true;
                let mut name_filter: Vec<OYarn> = vec![];
                for import_symbol in import_result.symbols {
                    if let Some(all) = session.st().get_content_symbol(import_symbol, "__all__", u32::MAX).symbols.first().copied() {
                        let all_value = SymbolTable::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                            all, None, false
                        )), session, None, false, true, None, None);
                        if let Some(all_value_first) = all_value.first() {
                            if !all_value_first.is_expired_if_weak(session.st()) {
                                let all_upgraded = all_value_first.upgrade_weak(session.st());
                                if let Some(all_upgraded_unwrapped) = all_upgraded {
                                    let evaluations = session.st().evaluations(all_upgraded_unwrapped);
                                    if let Some(evals) = evaluations && evals.len() == 1 {
                                        let value = &evals[0].value;
                                        if value.is_some() {
                                            let (nf, parse_error) = self.extract_all_symbol_eval_values(&value.as_ref());
                                            if parse_error {
                                                warn!("error during parsing __all__ import in file {}", session.st().paths(import_symbol)[0] )
                                            }
                                            name_filter = nf;
                                            all_name_allowed = false;
                                        } else {
                                            warn!("invalid __all__ import in file {} - no value found", session.st().paths(import_symbol)[0])
                                        }
                                    } else {
                                        warn!("invalid __all__ import in file {} - multiple evaluation found", session.st().paths(import_symbol)[0])
                                    }
                                } else {
                                    warn!("invalid __all__ import in file {} - localizedSymbol not found", session.st().paths(import_symbol)[0])
                                }
                            } else {
                                warn!("invalid __all__ import in file {} - expired symbol", session.st().paths(import_symbol)[0])
                            }
                        } else {
                            warn!("invalid __all__ import in file {} - no symbol found", session.st().paths(import_symbol)[0])
                        }
                    }
                    if !matches!(import_symbol, SymbolKey::Compiled(_) | SymbolKey::DiskDir(_) | SymbolKey::Root(_) | SymbolKey::Namespace(_)) // DISK_DIR does not have iter_symbols and is a symptom of unresolved dirs in custom EPs
                    && *self.sym_stack.last().unwrap() != import_symbol { /*We have to check that the imported symbol is not the current one. It can
                        happen for example in a .pyi that is importing the .pyd file with the same name. As both exists, odools will try to import the pyi a second time in the same file,
                        and so create a borrow error here
                        */
                        let mut import_variables_to_create = vec![];
                        for (name, loc_syms) in session.st().iter_symbols(import_symbol) {
                            if all_name_allowed || name_filter.contains(name) {
                                let evaluations = Evaluation::from_sections(session.st(), import_symbol, loc_syms);
                                import_variables_to_create.push((name.clone(), evaluations));
                            }
                        }
                        for (name, evaluations) in import_variables_to_create {
                            let evaluated_type = &evaluations[0].symbol;
                            let evaluated_type = evaluated_type.get_symbol_as_weak(session, None, &mut self.diagnostics, None).weak;
                            if let Some(evaluated_type) = evaluated_type.upgrade(session.st()) {
                                let evaluated_type_file = session.st().get_file(evaluated_type).unwrap();
                                if !(self.file == evaluated_type_file) {
                                    session.st_mut().add_dependency(self.file, evaluated_type_file, self.current_step, BuildSteps::ARCH);
                                }
                            }
                            let variable_key = session.st_mut().add_new_variable(*self.sym_stack.last().unwrap(), &name, import_result.range);
                            let variable = &mut session.st_mut()[variable_key];
                            variable.is_import_variable = true;
                            variable.evaluations = evaluations;
                        }
                    }
                }
            } else {
                let var_name = if let Some(asname) = &import_name.asname {
                    asname.as_str()
                } else {
                    import_name.name.split(".").next().unwrap()
                };
                let variable_key = session.st_mut().add_new_variable(*self.sym_stack.last().unwrap(), var_name, import_name.range);
                session.st_mut()[variable_key].is_import_variable = true;
            }
        }
    }

    fn visit_node(&mut self, session: &mut SessionInfo, nodes: &[Stmt]) {
        for stmt in nodes.iter() {
            match stmt {
                Stmt::Import(import_stmt) => {
                    self.create_local_symbols_from_import_stmt(session, None, &import_stmt.names, 0, &import_stmt.range);
                },
                Stmt::ImportFrom(import_from_stmt) => {
                    if import_from_stmt.module.is_none() && import_from_stmt.level == 0 {
                        continue;
                    }
                    self.create_local_symbols_from_import_stmt(session, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level, &import_from_stmt.range)
                },
                Stmt::AnnAssign(ann_assign_stmt) => {
                    self._visit_ann_assign(session, ann_assign_stmt);
                },
                Stmt::Assign(assign_stmt) => {
                    self._visit_assign(session, assign_stmt);
                },
                Stmt::FunctionDef(function_def_stmt) => {
                    self.visit_func_def(session, function_def_stmt);
                },
                Stmt::ClassDef(class_def_stmt) => {
                    self.visit_class_def(session, class_def_stmt);
                },
                Stmt::If(if_stmt) => {
                    self.visit_if(session, if_stmt);
                },
                Stmt::Try(try_stmt) => {
                    self.visit_try(session, try_stmt);
                },
                Stmt::For(for_stmt) => {
                    self.visit_for(session, for_stmt);
                },
                Stmt::With(with_stmt) => {
                    self.visit_with(session, with_stmt);
                },
                Stmt::Match(match_stmt) => {
                    self.visit_match(session, match_stmt);
                },
                Stmt::While(while_stmt) => {
                    self.visit_while(session, while_stmt);
                },
                Stmt::Expr(stmt_expression) => {
                    self.visit_expr(session, &stmt_expression.value);
                },
                Stmt::Return(return_stmt) => {
                    if let Some(value) = return_stmt.value.as_ref() {
                        self.visit_expr(session, value);
                    }
                },
                Stmt::Assert(assert_stmt) => {
                    self.visit_expr(session, &assert_stmt.test);
                    // `assert isinstance(x, T)` narrows the rest of the current block
                    let scope = *self.sym_stack.last().unwrap();
                    let after_assert = assert_stmt.range().end() + TextSize::new(1);
                    session.st_mut().as_mut_symbol_mgr(scope).add_section(after_assert, None);
                    self.declare_isinstance_narrowing(session, scope, &assert_stmt.test, after_assert);
                },
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
                    if let Some(stmt_exc) = stmt_raise.exc.as_ref() { self.visit_expr(session, stmt_exc) }
                    if let Some(stmt_cause) = stmt_raise.cause.as_ref() { self.visit_expr(session, stmt_cause) }
                },
                Stmt::Global(_stmt_global) => {},
                Stmt::Nonlocal(_stmt_nonlocal) => {},
                Stmt::Break(_) => {},
                Stmt::Continue(_) => {},
                Stmt::Pass(_) => {},
                Stmt::IpyEscapeCommand(_) => {},
            }
        }
    }

    fn visit_expr(&mut self, session: &mut SessionInfo, expr: &Expr){
        match expr {
            Expr::Named(named_expr) =>{
                self.visit_named_expr(session, named_expr);
            },
            Expr::BoolOp(bool_op_expr) => {
                // introduce sections here
                // Due to short circuit behavior
                // Further conditions can be skipped
                // Which could have named expressions

                // one section per value
                // one succeeding section with all the value sections in OR
                let scope = *self.sym_stack.last().unwrap();
                let mut prev_section = session.st().as_symbol_mgr(scope).get_last_index();
                let mut prev_operand: Option<&Expr> = None;
                let cond_sections = bool_op_expr.values.iter().map(|expr|{
                    session.st_mut().as_mut_symbol_mgr(scope).add_section(
                        expr.range().start(),
                        Some(SectionIndex::INDEX(prev_section))
                    );
                    // sections for short-circuiting and operations
                    if matches!(bool_op_expr.op, BoolOp::And)
                        && let Some(prev) = prev_operand {
                            self.declare_isinstance_narrowing(session, scope, prev, expr.range().start());
                        }
                    self.visit_expr(session, expr);
                    prev_section = session.st().as_symbol_mgr(scope).get_last_index();
                    prev_operand = Some(expr);
                    SectionIndex::INDEX(prev_section)
                }).collect::<Vec<_>>();
                session.st_mut().as_mut_symbol_mgr(scope).add_section(
                    bool_op_expr.range().end() + TextSize::new(1),
                    Some(SectionIndex::OR(cond_sections))
                );
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
                        if let Some(dict_key_expr) = dict_item.key.as_ref() { self.visit_expr(session, dict_key_expr) }
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
                if let Some(yield_value) = expr_yield.value.as_ref() { self.visit_expr(session, yield_value) }
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
                if let Some(upper_expr) = expr_slice.upper.as_ref() { self.visit_expr(session, upper_expr) }
                if let Some(lower_expr) = expr_slice.lower.as_ref() { self.visit_expr(session, lower_expr) }
            },
            // Expressions that cannot contained a named expressions are not traversed
            Expr::Lambda(lambda_expr) => {
                let function_key = session.st_mut().add_new_function(
                    *self.sym_stack.last().unwrap(), &S!("<lambda>"), lambda_expr.range, lambda_expr.body.range().start()
                );
                //arch is considered done on the fly
                session.st_mut().set_build_status(function_key.into(), BuildSteps::ARCH, BuildStatus::DONE);
                if let Some(parameters) = &lambda_expr.parameters {
                    PythonArchBuilder::handle_func_args(function_key, session, parameters);
                }
                self.sym_stack.push(function_key.into());
                self.visit_expr(session, &lambda_expr.body);
                self.sym_stack.pop();
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

    fn extract_all_symbol_eval_values(&self, value: &Option<&EvaluationValue>) -> (Vec<OYarn>, bool) {
        let mut parse_error = false;
        let vec: Vec<OYarn> = match value {
            Some(eval) => {
                match eval {
                    EvaluationValue::ANY() => {
                        parse_error = true;
                        vec![]
                    }
                    EvaluationValue::CONSTANT(c) => {
                        match &**c {
                            Expr::StringLiteral(s) => {
                                vec![oyarn!("{}", s.value)]
                            },
                            _ => {parse_error = true; vec![]}
                        }
                    },
                    EvaluationValue::DICT(_d) => {
                        parse_error = true; vec![]
                    },
                    EvaluationValue::LIST(l) => {
                        let mut res = vec![];
                        for v in l.iter() {
                            match v {
                                Expr::StringLiteral(s) => {
                                    res.push(oyarn!("{}", s.value));
                                }
                                _ => {parse_error = true; }
                            }
                        }
                        res
                    },
                    EvaluationValue::TUPLE(t) => {
                        let mut res = vec![];
                        for v in t.iter() {
                            match v {
                                Expr::StringLiteral(s) => {
                                    res.push(oyarn!("{}", s.value));
                                }
                                _ => {parse_error = true; }
                            }
                        }
                        res
                    }
                }
            },
            None => {parse_error = true; vec![]}
        };
        (vec, parse_error)
    }

    fn _visit_ann_assign(&mut self, session: &mut SessionInfo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&[*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&[*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        for assign in assigns.iter() { //should only be one
            if let Some(ref expr) = assign.value{
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    session.sync_odoo.symbol_table.add_new_variable(*self.sym_stack.last().unwrap(), &name_expr.id, name_expr.range);
                },
                AssignTargetType::Attribute(ref _attr_expr) => {
                }
            }
        }
    }

    fn _visit_assign(&mut self, session: &mut SessionInfo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        for assign in assigns.iter() {
            if let Some(ref expr) = assign.value {
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    let variable_key = session.st_mut().add_new_variable(*self.sym_stack.last().unwrap(), &name_expr.id, name_expr.range);
                    let variable = &session.st()[variable_key];
                    if self.file_mode && variable.name == "__all__"
                        && let Some(value) = &assign.value
                    {
                        let mut deps = vec![vec![]]; //only arch level
                        let eval = Evaluation::eval_from_ast(session, value, variable.parent(), &assign_stmt.range.start(), false, &mut deps);
                        session.st_mut().insert_dependencies(self.file, &deps, BuildSteps::ARCH);
                        session.st_mut()[variable_key].evaluations = eval.0;
                        self.diagnostics.extend(eval.1);
                        if let Some(evaluation) = session.st()[variable_key].evaluations.first()
                            && session.st().is_external(*self.sym_stack.last().unwrap()) {
                                // external packages often import symbols from compiled files
                                // or with meta programmation like globals["var"] = __get_func().
                                // we don't want to handle that, so just declare __all__ content
                                // as symbols to not raise any error.
                                if let Some(EvaluationValue::LIST(list)) = &evaluation.value {
                                    for item in list.iter() {
                                        if let Expr::StringLiteral(s) = item {
                                            self.__all_symbols_to_add.push((s.value.to_string(), evaluation.range.unwrap()));
                                        }
                                    }
                                }
                            }
                    }
                },
                AssignTargetType::Attribute(ref _attr_expr) => {
                    //take base evals
                    // let mut required_dependencies = if self.file_mode {
                    //     vec![vec![], vec![]] //arch level and eval level
                    // } else {
                    //     vec![vec![]] //only arch level
                    // };
                    // let (base_evals, diags) = Evaluation::eval_from_ast(session, &attr_expr.value, parent.clone(), &attr_expr.range.start(), &mut required_dependencies);
                    // if base_evals.len() == 1 {
                    //     //check that the attribute doesn't already exists
                    //     let base_ref = base_eval.symbol.get_symbol(session, context, &mut diagnostics, Some(parent.clone()));
                    //     if base_ref.is_expired_if_weak() {
                    //         return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                    //     }
                    //     let bases = Symbol::follow_ref(&base_ref, session, context, false, false, None, &mut diagnostics);
                    //     for ibase in bases.iter() {
                    //         let base_loc = ibase.upgrade_weak();
                    //         if let Some(base_loc) = base_loc {
                    //             let file = base_loc.borrow().get_file().clone();
                    //             if let Some(base_loc_file) = file {
                    //                 let base_loc_file = base_loc_file.upgrade().unwrap();
                    //                 SyncOdoo::build_now(session, &base_loc_file, BuildSteps::ARCH_EVAL);
                    //                 if base_loc_file.borrow().in_workspace() {
                    //                     if required_dependencies.len() == 2 {
                    //                         required_dependencies[1].push(base_loc_file.clone());
                    //                     } else if required_dependencies.len() == 3 {
                    //                         required_dependencies[2].push(base_loc_file.clone());
                    //                     }
                    //                 }
                    //             }
                    //             let is_super = ibase.is_weak() && ibase.as_weak().is_super;
                    //             let (attributes, mut attributes_diagnostics) = base_loc.borrow().get_member_symbol(session, &expr.attr.to_string(), module.clone(), false, false, true, is_super);
                    //             for diagnostic in attributes_diagnostics.iter_mut(){
                    //                 diagnostic.range = FileMgr::textRange_to_temporary_Range(&expr.range())
                    //             }
                    //             diagnostics.extend(attributes_diagnostics);
                    //             if !attributes.is_empty() {
                    //                 let is_instance = ibase.as_weak().instance.unwrap_or(false);
                    //                 attributes.iter().for_each(|attribute|{
                    //                     let mut eval = Evaluation::eval_from_symbol(&Rc::downgrade(attribute), None);
                    //                     match eval.symbol.sym {
                    //                         EvaluationSymbolPtr::WEAK(ref mut weak) => {
                    //                             weak.context.insert(S!("base_attr"), ContextValue::SYMBOL(Rc::downgrade(&base_loc)));
                    //                             weak.context.insert(S!("is_attr_of_instance"), ContextValue::BOOLEAN(is_instance));
                    //                         },
                    //                         _ => {}
                    //                     }
                    //                     evals.push(eval);
                    //                 });
                    //             }
                    //         }
                    //     }
                    // }
                }
            }
        }
    }

    fn visit_named_expr(&mut self, session: &mut SessionInfo, named_expr: &ExprNamed) {
        self.visit_expr(session, &named_expr.value);
        if let Some(name_expr) = named_expr.target.as_name_expr() { // Only handle valid named expressions
            session.sync_odoo.symbol_table.add_new_variable(*self.sym_stack.last().unwrap(), &name_expr.id, named_expr.target.range());
        }
    }

    fn handle_func_args(function_key: FunctionKey, session: &mut SessionInfo, parameters: &Parameters) {
        for arg in parameters.posonlyargs.iter() {
            let param = session.st_mut().add_new_variable(function_key, &arg.parameter.name.id, arg.range);
            session.st_mut()[param].is_parameter = true;
            let mut default = None;
            if arg.default.is_some() {
                default = Some(Evaluation::new_none()); //TODO evaluate default? actually only used to know if there is a default or not
            }
            session.st_mut()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: default,
                arg_type: ArgumentType::POS_ONLY,
                annotation: arg.parameter.annotation.clone(),
            });
        }
        for arg in parameters.args.iter() {
            let param = session.st_mut().add_new_variable(function_key, &arg.parameter.name.id, arg.range);
            session.st_mut()[param].is_parameter = true;
            let mut default = None;
            if arg.default.is_some() {
                default = Some(Evaluation::new_none()); //TODO evaluate default? actually only used to know if there is a default or not
            }
            session.st_mut()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: default,
                arg_type: ArgumentType::ARG,
                annotation: arg.parameter.annotation.clone(),
            });
        }
        if let Some(arg) = &parameters.vararg {
            let param = session.st_mut().add_new_variable(function_key, &arg.name.id, arg.range);
            session.st_mut()[param].is_parameter = true;
            session.st_mut()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: None,
                arg_type: ArgumentType::VARARG,
                annotation: arg.annotation.clone(),
            });
        }
        for arg in parameters.kwonlyargs.iter() {
            let param = session.st_mut().add_new_variable(function_key, &arg.parameter.name.id, arg.range);
            session.st_mut()[param].is_parameter = true;
            session.st_mut()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: arg.default.as_ref().map(|_default| Evaluation::new_none()),
                arg_type: ArgumentType::KWORD_ONLY,
                annotation: arg.parameter.annotation.clone(),
            });
        }
        if let Some(arg) = &parameters.kwarg {
            let param = session.st_mut().add_new_variable(function_key, &arg.name.id, arg.range);
            session.st_mut()[param].is_parameter = true;
            session.st_mut()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: None,
                arg_type: ArgumentType::KWARG,
                annotation: arg.annotation.clone(),
            });
        }
    }

    fn visit_func_def(&mut self, session: &mut SessionInfo, func_def: &StmtFunctionDef) {
        if func_def.body.is_empty() {
            return; //if body is empty, it usually means that the ast of the class is invalid. Skip it
        }
        let function_key = session.st_mut().add_new_function(*self.sym_stack.last().unwrap(),
            &func_def.name.id, func_def.range, func_def.body.first().unwrap().range().start());
        let func_sym = &mut session.st_mut()[function_key];
        func_sym.node_index.set(func_def.node_index.load());
        for decorator in func_def.decorator_list.iter() {
            if let Some(name) = decorator.expression.as_name_expr() {
                match name.id.as_str() {
                    "staticmethod" => func_sym.is_static = true,
                    "property" | "cached_property" | "lazy_property" => func_sym.is_property = true,
                    "overload" => func_sym.is_overloaded = true,
                    "classmethod" => func_sym.is_class_method = true,
                    "classproperty" | "lazy_classproperty" => {
                        func_sym.is_property = true;
                        func_sym.is_class_method = true;
                    },
                    _ => {}
                }
            }
            // decorators can also be reached through their module, e.g. functools.cached_property,
            // tools.lazy_property, and typing.overload as odoo's orm uses it
            else if let Some(attr) = decorator.expression.as_attribute_expr() {
                match attr.attr.id.as_str() {
                    "cached_property" | "lazy_property" => func_sym.is_property = true,
                    "overload" => func_sym.is_overloaded = true,
                    _ => {}
                }
            }
        }
        // __init_subclass__ and __class_getitem__ are always classmethods
        // see https://docs.python.org/3/reference/datamodel.html
        if ["__init_subclass__", "__class_getitem__"].contains(&func_sym.name.as_str()) {
            func_sym.is_class_method = true;
        }
        if func_def.body[0].is_expr_stmt() {
            let expr: &ruff_python_ast::StmtExpr = func_def.body[0].as_expr_stmt().unwrap();
            if let Some(s) = expr.value.as_string_literal_expr() {
                func_sym.doc_string = Some(s.value.to_string())
            }
        }
        //add params
        PythonArchBuilder::handle_func_args(function_key, session, &func_def.parameters);
        let mut add_noqa = false;
        if let Some(noqa_bloc) = self.file_info.as_ref().unwrap().borrow().noqas_blocs.get(&func_def.range.start().to_u32()) {
            session.noqas_stack.push(noqa_bloc.clone());
            add_noqa = true;
        }
        let noqa = combine_noqa_info(&session.noqas_stack);
        session.st_mut()[function_key].noqas = noqa.clone();
        session.current_noqa = noqa;
        //visit body
        if !self.file_mode || session.st().get_in_parents(function_key.into(), &[SymType::CLASS], true).is_none() {
            session.st_mut()[function_key].set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
            self.sym_stack.push(function_key.into());
            self.visit_node(session, &func_def.body);
            self.sym_stack.pop();
            session.st_mut().set_build_status(function_key.into(), BuildSteps::ARCH, BuildStatus::DONE);
        }
        if add_noqa {
            session.noqas_stack.pop();
        }
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, class_def: &StmtClassDef) {
        if class_def.body.is_empty() {
            return; //if body is empty, it usually means that the ast of the class is invalid. Skip it
        }
        let parent = *self.sym_stack.last().unwrap();
        let class_key = session.st_mut().add_new_class(
            parent, class_def.name.id.as_str(), class_def.range, class_def.body.first().unwrap().range().start());
        let class_sym = &mut session.sync_odoo.symbol_table[class_key];

        if !class_def.body.is_empty() && class_def.body[0].is_expr_stmt() {
            let expr = class_def.body[0].as_expr_stmt().unwrap();
            if expr.value.is_literal_expr() {
                let const_expr = expr.value.as_literal_expr().unwrap();
                if let Some(s) = const_expr.as_string_literal() {
                    class_sym.doc_string = Some(s.value.to_string());
                }
            }
        }
        let mut add_noqa = false;
        if let Some(noqa_bloc) = self.file_info.as_ref().unwrap().borrow().noqas_blocs.get(&class_def.range.start().to_u32()) {
            session.noqas_stack.push(noqa_bloc.clone());
            add_noqa = true;
        }
        let noqas = combine_noqa_info(&session.noqas_stack);
        class_sym.noqas = noqas.clone();
        session.current_noqa = noqas;
        self.sym_stack.push(class_key.into());
        self.visit_node(session, &class_def.body);
        self.sym_stack.pop();
        if add_noqa {
            session.noqas_stack.pop();
        }
        PythonArchBuilderHooks::on_class_def(session, class_key);
    }

    fn _resolve_all_symbols(&mut self, session: &mut SessionInfo) {
        let parent = *self.sym_stack.last().unwrap();
        for (symbol_name, range) in self.__all_symbols_to_add.drain(..) {
            if session.st().get_content_symbol(parent, &symbol_name, u32::MAX).symbols.is_empty() {
                session.st_mut().add_new_variable(parent, &symbol_name, range);
            }
        }
    }

    fn check_tuples(&self, version: &[u32], op: &CmpOp, tuple: &ExprTuple) -> bool {
        let mut tuple = tuple.elts.iter().map(|elt| {
            if let Expr::NumberLiteral(num) = elt {
                if num.value.is_int() {
                    num.value.as_int().unwrap().as_u32().unwrap()
                } else {
                    0_u32
                }
            } else {
                0_u32 // If not a number, treat as 0
            }
        }).collect::<Vec<u32>>();
        // ensure that the vec is sized of 3
        tuple.resize(3, 0);
        match op {
            CmpOp::Gt => {
                version[0] > tuple[0] ||
                (version[0] == tuple[0] && version[1] > tuple[1]) ||
                (version[0] == tuple[0] && version[1] == tuple[1] && version[2] > tuple[2])
            },
            CmpOp::GtE => {
                version[0] >= tuple[0] ||
                (version[0] == tuple[0] && version[1] >= tuple[1]) ||
                (version[0] == tuple[0] && version[1] == tuple[1] && version[2] >= tuple[2])
            },
            CmpOp::Lt => {
                version[0] < tuple[0] ||
                (version[0] == tuple[0] && version[1] < tuple[1]) ||
                (version[0] == tuple[0] && version[1] == tuple[1] && version[2] < tuple[2])
            },
            CmpOp::LtE => {
                version[0] <= tuple[0] ||
                (version[0] == tuple[0] && version[1] <= tuple[1]) ||
                (version[0] == tuple[0] && version[1] == tuple[1] && version[2] <= tuple[2])
            },
            CmpOp::Eq => {
                version[0] == tuple[0] &&
                version[1] == tuple[1] &&
                version[2] == tuple[2]
            },
            CmpOp::NotEq => {
                version[0] != tuple[0] ||
                version[1] != tuple[1] ||
                version[2] != tuple[2]
            },
            _ => {
                false
            }
        }
    }

    /** returns
    * first bool: true if we can go in the condition, because no version check is preventing it
    * second bool: true if there was a version check or false if the condition was unrelated
    */
    fn _check_sys_version_condition(&self, session: &mut SessionInfo, expr: &Expr) -> (bool, bool) {
        if session.sync_odoo.python_version[0] == 0 {
            return (true, false); //unknown python version
        }
        if let Expr::Compare(expr_comp) = expr
            && expr_comp.comparators.len() == 1 {
                let p1 = expr_comp.left.as_ref();
                let p2 = expr_comp.comparators.first().unwrap();
                if !p1.is_tuple_expr() && !p2.is_tuple_expr() {
                    return (true, false);
                }
                if !p1.is_attribute_expr() && !p2.is_attribute_expr() {
                    return (true, false);
                }
                let (tuple, attr) = if p1.is_tuple_expr() {
                    (p1.as_tuple_expr().unwrap(), p2.as_attribute_expr().unwrap())
                } else {
                    (p2.as_tuple_expr().unwrap(), p1.as_attribute_expr().unwrap())
                };
                if attr.value.is_name_expr() && attr.value.as_name_expr().unwrap().id == "sys"
                    && attr.attr.id == "version_info" {
                        let mut op = expr_comp.ops.first().unwrap();
                        if p1.is_tuple_expr() { //invert if tuple is in front
                            if op.is_gt() {
                                op = &CmpOp::Lt;
                            } else if op.is_gt_e() {
                                op = &CmpOp::LtE;
                            } else if op.is_lt() {
                                op = &CmpOp::Gt;
                            } else if op.is_lt_e() {
                                op = &CmpOp::GtE;
                            }
                        }
                        return (self.check_tuples(&session.sync_odoo.python_version, op, tuple), true)
                    }
            }
        (true, false)
    }

    /// Declare a synthetic narrowing declaration for `check.target_name` at the start of a body.
    fn declare_narrowing_for_check(&self, session: &mut SessionInfo, scope: SymbolKey, check: &IsinstanceCheck, test_start: TextSize, body_start: TextSize) {
        // `narrowed_from` lets consumers (go-to-definition, find-references) look through this
        // bookkeeping node to the real declaration.
        let shadowed = SymbolTable::infer_name(session.sync_odoo, scope, check.target_name, Some(test_start.to_u32()));
        let narrowed_from: Vec<Wk<SymbolKey>> = shadowed.symbols.into_iter().map(Wk::from).collect();
        let variable_key = session.st_mut().add_new_variable(scope, check.target_name, narrowing_range(body_start));
        session.st_mut()[variable_key].narrowed_from = narrowed_from;
    }

    /// If `test` is a (non-negated) `isinstance(x, T)` check on a plain name, declare the
    /// narrowing at `body_start`
    fn declare_isinstance_narrowing(&self, session: &mut SessionInfo, scope: SymbolKey, test: &Expr, body_start: TextSize) {
        let Some(check) = match_isinstance_check(test) else { return };
        if check.negated {
            return;
        }
        self.declare_narrowing_for_check(session, scope, &check, test.range().start(), body_start);
    }

    /// If `test` is a *negated* `not isinstance(x, T)` check, declare the (positive) narrowing
    /// at `false_branch_start` - the section reached when `test` was false.
    fn declare_negated_isinstance_narrowing(&self, session: &mut SessionInfo, scope: SymbolKey, test: &Expr, false_branch_start: TextSize) {
        let Some(check) = match_isinstance_check(test) else { return };
        if !check.negated {
            return;
        }
        self.declare_narrowing_for_check(session, scope, &check, test.range().start(), false_branch_start);
    }

    /// Whether `body` always ends in `return`/`raise`/`continue`/`break` - not exhaustive (e.g.
    /// a terminating nested `if`/`else`), but covers the common early-exit guard.
    fn body_always_exits(body: &[Stmt]) -> bool {
        matches!(body.last(), Some(Stmt::Return(_) | Stmt::Raise(_) | Stmt::Continue(_) | Stmt::Break(_)))
    }

    /// If `test` is an `and`-chain, the section of its *last* operand - the correct predecessor
    /// for a body that only runs once the whole chain was true. The chain's generic merge
    /// section also accounts for short-circuit exits, so using it here would union the true
    /// path's narrowing back with the pre-narrowing state.
    fn and_chain_last_operand_section(&self, session: &mut SessionInfo, scope: SymbolKey, test: &Expr) -> Option<SectionIndex> {
        let Expr::BoolOp(bool_op) = test else { return None };
        if !matches!(bool_op.op, BoolOp::And) {
            return None;
        }
        let last = bool_op.values.last()?;
        let index = session.st().as_symbol_mgr(scope).get_section_for(last.range().start().to_u32()).index;
        Some(SectionIndex::INDEX(index))
    }

    fn visit_if(&mut self, session: &mut SessionInfo, if_stmt: &StmtIf) {
        //TODO check platform condition (sys.version > 3.12, etc...)
        let scope = *self.sym_stack.last().unwrap();
        let scope_as_sym_mgr = session.st_mut().as_mut_symbol_mgr(scope);
        let prefix_section = scope_as_sym_mgr.get_last_index();
        let test_section = scope_as_sym_mgr.add_section(
            if_stmt.test.range().start(),
            None // Take preceding section (before if stmt)
        );
        let mut last_test_section = test_section.index;
        let mut last_test: &Expr = if_stmt.test.as_ref();

        self.visit_expr(session, &if_stmt.test);
        let body_prev = self.and_chain_last_operand_section(session, scope, if_stmt.test.as_ref());
        let mut body_version_ok = false; //if true, it means we found a condition that is true and contained a version check. Used to avoid else clause
        let mut stmt_sections = if if_stmt.body.is_empty() {
            vec![]
        } else {
            session.st_mut().as_mut_symbol_mgr(scope).add_section( // first body section
                if_stmt.body[0].range().start(),
                body_prev // `None` unless the test is an `and`-chain (see `and_chain_last_operand_section`)
            );
            self.declare_isinstance_narrowing(session, scope, &if_stmt.test, if_stmt.body[0].range().start());
            let check_version = self._check_sys_version_condition(session, if_stmt.test.as_ref());
            if check_version.0 {
                if check_version.1 {
                    body_version_ok = true;
                }
                self.visit_node(session, &if_stmt.body);
                if Self::body_always_exits(&if_stmt.body) {
                    vec![]
                } else {
                    vec![ SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index())]
                }
            } else {
                vec![]
            }
        };

        let mut else_clause_exists = false;

        let stmt_clauses_iter = if_stmt.elif_else_clauses.iter().filter_map(|elif_else_clause|{
            match elif_else_clause.test {
                Some(ref test_clause) => {
                    last_test_section = session.st_mut().as_mut_symbol_mgr(scope).add_section(
                        test_clause.range().start(),
                        Some(SectionIndex::INDEX(last_test_section))
                    ).index;
                    // Reaching this test means the previous one was false - narrow it here (not
                    // just at the final fallthrough) so it propagates to everything after.
                    self.declare_negated_isinstance_narrowing(session, scope, last_test, test_clause.range().start());
                    self.visit_expr(session, test_clause);
                    last_test = test_clause;
                },
                None => else_clause_exists = true
            }
            if elif_else_clause.body.is_empty() {
                return None;
            }
            session.st_mut().as_mut_symbol_mgr(scope).add_section(
                elif_else_clause.body[0].range().start(),
                Some(SectionIndex::INDEX(last_test_section))
            );
            if let Some(test_clause) = &elif_else_clause.test {
                self.declare_isinstance_narrowing(session, scope, test_clause, elif_else_clause.body[0].range().start());
            }
            if let Some(test_clause) = &elif_else_clause.test {
                let version_check = self._check_sys_version_condition(session, test_clause);
                if version_check.0 {
                    if version_check.1 {
                        body_version_ok = true;
                    }
                    self.visit_node(session, &elif_else_clause.body);
                }
            }
            else if !body_version_ok { //else clause
                self.visit_node(session, &elif_else_clause.body);
            }
            if Self::body_always_exits(&elif_else_clause.body) {
                None
            } else {
                Some(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()))
            }
        });

        stmt_sections.extend(stmt_clauses_iter);

        if !else_clause_exists{
            // Implicit else: goes from the last test to out of the if-statement
            // + 1 to avoid section collision
            let false_branch_start = if_stmt.range().end() + TextSize::new(1);
            session.st_mut().as_mut_symbol_mgr(scope).add_section(
                false_branch_start,
                Some(SectionIndex::INDEX(last_test_section))
            );
            self.declare_negated_isinstance_narrowing(session, scope, last_test, false_branch_start);
            stmt_sections.push(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()));
        }
        if stmt_sections.is_empty(){
            // If there are no valid bodies or tests, point to the section before the if-stmt
            stmt_sections.push(SectionIndex::INDEX(prefix_section));
        }
        session.st_mut().as_mut_symbol_mgr(scope).add_section(
            if_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
    }

    fn visit_for(&mut self, session: &mut SessionInfo, for_stmt: &StmtFor) {
        // TODO: Handle breaks for sections
        let scope = *self.sym_stack.last().unwrap();
        let unpacked = python_utils::unpack_assign(&[*for_stmt.target.clone()], None, None);
        for assign in unpacked {
            if let Some(ref expr) = assign.value {
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    session.st_mut().add_new_variable(scope, &name_expr.id, name_expr.range);
                },
                AssignTargetType::Attribute(_) => {
                }
            }
        }
        let scope_as_sym_mgr = session.st_mut().as_mut_symbol_mgr(scope);
        let previous_section = SectionIndex::INDEX(scope_as_sym_mgr.get_last_index());
        if let Some(first_body_stmt) = for_stmt.body.first() {
            scope_as_sym_mgr.add_section(
                first_body_stmt.range().start(),
                None
            );
        }

        self.visit_node(session, &for_stmt.body);
        let mut stmt_sections = vec![SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index())];

        if !for_stmt.orelse.is_empty(){
            session.st_mut().as_mut_symbol_mgr(scope).add_section(
                for_stmt.orelse[0].range().start(),
                Some(previous_section.clone())
            );
            self.visit_node(session, &for_stmt.orelse);
            stmt_sections.push(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()));
        } else {
            stmt_sections.push(previous_section.clone());
        }

        session.st_mut().as_mut_symbol_mgr(scope).add_section(
            for_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
    }

    fn visit_try(&mut self, session: &mut SessionInfo, try_stmt: &StmtTry) {
        // Try sections:
        // try block is always executed, so it has the same section as the one preceding it.
        // Finally is always executed if it exists, so it belongs to the lower section
        let scope = *self.sym_stack.last().unwrap();
        self.visit_node(session, &try_stmt.body);
        if !try_stmt.handlers.is_empty(){
            // Branching around except _T, except, and else act similar to if-elif-else
            // The direct link (eq. to empty section) to previous scope is always there
            // Unless both catch-all except and else clauses exist.
            let previous_section = SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index());
            let mut stmt_sections = vec![previous_section.clone()];
            let mut catch_all_except_exists = false;
            for handler in try_stmt.handlers.iter() {
                match handler {
                    ruff_python_ast::ExceptHandler::ExceptHandler(h) => {
                        if !catch_all_except_exists { catch_all_except_exists = h.type_.is_none()};
                        if h.body.is_empty() {
                            continue;
                        }
                        session.st_mut().as_mut_symbol_mgr(scope).add_section(
                            h.body[0].range().start(),
                            Some(previous_section.clone())
                        );
                        self.visit_node(session, &h.body);
                        stmt_sections.push(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()));
                    }
                }
            }
            if !try_stmt.orelse.is_empty(){
                if catch_all_except_exists{
                    stmt_sections.remove(0);
                }
                session.st_mut().as_mut_symbol_mgr(scope).add_section(
                    try_stmt.orelse[0].range().start(),
                    Some(previous_section.clone())
                );
                self.visit_node(session, &try_stmt.orelse);
                stmt_sections.push(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()));
            }
            // Next section is either the start of the finally block, or right after the try block if finally does not exist
            let next_section_start = try_stmt.finalbody.first().map(|stmt| stmt.range().start()).unwrap_or(try_stmt.range().end() + TextSize::new(1));
            session.st_mut().as_mut_symbol_mgr(scope).add_section(
                next_section_start,
                Some(SectionIndex::OR(stmt_sections))
            );
        }
        self.visit_node(session, &try_stmt.finalbody);
    }

    fn visit_with(&mut self, session: &mut SessionInfo, with_stmt: &StmtWith) {
        for item in with_stmt.items.iter() {
            self.visit_expr(session, &item.context_expr);
            if let Some(var) = item.optional_vars.as_ref() {
                match &**var {
                    Expr::Name(expr_name) => {
                        let parent = *self.sym_stack.last().unwrap();
                        session.sync_odoo.symbol_table.add_new_variable(parent, &expr_name.id, var.range());
                    },
                    Expr::Tuple(_) => {continue;},
                    Expr::List(_) => {continue;},
                    _ => {continue;}
                }
            }
        }
        self.visit_node(session, &with_stmt.body);
    }

    fn visit_match(&mut self, session: &mut SessionInfo, match_stmt: &StmtMatch) {
        fn traverse_match(pattern: &Pattern, symbol_table: &mut SymbolTable, scope: SymbolKey){
            match pattern {
                Pattern::MatchValue(_) => {},
                Pattern::MatchSingleton(_) => {},
                Pattern::MatchSequence(match_sequence) => {
                    match_sequence.patterns.iter().for_each(|sequence_pattern| traverse_match(sequence_pattern, symbol_table, scope));
                },
                Pattern::MatchMapping(match_mapping) => {
                    match_mapping.patterns.iter().for_each(|mapping_value_pattern| traverse_match(mapping_value_pattern, symbol_table, scope));
                },
                Pattern::MatchClass(match_class) => {
                    match_class.arguments.patterns.iter().for_each(|class_arg_pattern| traverse_match(class_arg_pattern, symbol_table, scope));
                },
                Pattern::MatchStar(pattern_match_star) => {
                    if let Some(name) = &pattern_match_star.name { //if name is None, this is a wildcard pattern (*_)
                        symbol_table.add_new_variable(
                            scope, name, pattern_match_star.range());
                    }
                },
                Pattern::MatchAs(pattern_match_as) => {
                    if let Some(name) = &pattern_match_as.name { //if name is None, this is a wildcard pattern (_)
                        symbol_table.add_new_variable(
                            scope, name, pattern_match_as.range());
                    }
                },
                Pattern::MatchOr(match_or) => {
                    match_or.patterns.iter().for_each(|pattern| traverse_match(pattern, symbol_table, scope));
                },
            }
        }

        let scope = *self.sym_stack.last().unwrap();
        let previous_section = SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index());
        let mut stmt_sections = vec![previous_section.clone()];
        for case in match_stmt.cases.iter() {
            if let Some(test_clause) = case.guard.as_ref() { self.visit_expr(session, test_clause) }
            if matches!(&case.pattern, ruff_python_ast::Pattern::MatchAs(_)){
                stmt_sections.remove(0); // When we have a wildcard pattern, previous section is shadowed
            }
            session.st_mut().as_mut_symbol_mgr(scope).add_section(
                case.range().start(),
                Some(previous_section.clone())
            );
            traverse_match(&case.pattern, session.st_mut(), scope);
            self.visit_node(session, &case.body);
            stmt_sections.push(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()));
        }
        session.st_mut().as_mut_symbol_mgr(scope).add_section(
            match_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
    }

    fn visit_while(&mut self, session: &mut SessionInfo, while_stmt: &StmtWhile) {
        // TODO: Handle breaks for sections
        let scope = *self.sym_stack.last().unwrap();
        let scope_as_sym_mgr = session.st_mut().as_mut_symbol_mgr(scope);
        let previous_section = SectionIndex::INDEX(scope_as_sym_mgr.get_last_index());
        if let Some(first_body_stmt) = while_stmt.body.first() {
            scope_as_sym_mgr.add_section(
                first_body_stmt.range().start(),
                None
            );
        }
        self.visit_expr(session, &while_stmt.test);
        if let Some(first_body_stmt) = while_stmt.body.first() {
            self.declare_isinstance_narrowing(session, scope, &while_stmt.test, first_body_stmt.range().start());
        }
        self.visit_node(session, &while_stmt.body);
        let scope_as_sym_mgr = session.st_mut().as_mut_symbol_mgr(scope);
        let body_section = SectionIndex::INDEX(scope_as_sym_mgr.get_last_index());
        let mut stmt_sections = vec![body_section];

        // A normal (non-`break`) loop exit means the test was false - narrow it here too, same
        // as `if`'s negative guard. Must sit before `orelse` when present (sections need
        // increasing start positions), so anchor on its start rather than `while_stmt`'s own end.
        let false_branch_start = while_stmt.orelse.first().map(|s| s.range().start())
            .unwrap_or(while_stmt.range().end() + TextSize::new(1));
        session.st_mut().as_mut_symbol_mgr(scope).add_section(
            false_branch_start,
            Some(previous_section)
        );
        self.declare_negated_isinstance_narrowing(session, scope, &while_stmt.test, false_branch_start);
        let false_branch_section = SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index());

        if !while_stmt.orelse.is_empty(){
            session.st_mut().as_mut_symbol_mgr(scope).add_section(
                while_stmt.orelse[0].range().start(),
                Some(false_branch_section)
            );
            self.visit_node(session, &while_stmt.orelse);
            stmt_sections.push(SectionIndex::INDEX(session.st().as_symbol_mgr(scope).get_last_index()));
        } else {
            stmt_sections.push(false_branch_section);
        }

        session.st_mut().as_mut_symbol_mgr(scope).add_section(
            while_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
    }
}
