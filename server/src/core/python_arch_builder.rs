use std::rc::Rc;
use std::cell::RefCell;
use std::vec;
use anyhow::Error;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ruff_python_ast::{Alias, AnyRootNodeRef, CmpOp, Expr, ExprNamed, ExprTuple, FStringPart, Identifier, Pattern, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFor, StmtFunctionDef, StmtIf, StmtMatch, StmtTry, StmtWhile, StmtWith};
use lsp_types::Diagnostic;
use tracing::{trace, warn};

use crate::constants::{BuildStatus, BuildSteps, OYarn, PackageType, SymType, DEBUG_STEPS, DEBUG_STEPS_ONLY_INTERNAL};
use crate::core::python_utils;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::evaluation::{Evaluation, EvaluationValue};
use crate::core::python_arch_builder_hooks::PythonArchBuilderHooks;
use crate::core::symbols::symbol_table::{follow_ref, get_sym, SymbolKey, SymbolTable};
use crate::threads::SessionInfo;
use crate::{oyarn, S};

use super::entry_point::EntryPoint;
use super::evaluation::{EvaluationSymbolPtr, EvaluationSymbolWeak};
use super::file_mgr::{combine_noqa_info, FileInfo};
use super::import_resolver::ImportResult;
use super::odoo::SyncOdoo;
use super::python_utils::AssignTargetType;
use super::symbols::function_symbol::{Argument, ArgumentType};
use super::symbols::module_symbol::ModuleSymbol;
use super::symbols::symbol_mgr::SectionIndex;


#[derive(Debug)]
pub struct PythonArchBuilder {
    entry_point: Rc<RefCell<EntryPoint>>,
    // @arena: "file" could be a function. Consider renaming this.
    file: SymbolKey,
    file_mode: bool,
    current_step: BuildSteps,
    sym_stack: Vec<SymbolKey>,
    __all_symbols_to_add: Vec<(String, TextRange)>,
    diagnostics: Vec<Diagnostic>,
    file_info: Option<Rc<RefCell<FileInfo>>>,
}

impl PythonArchBuilder {
    pub fn new(entry_point: Rc<RefCell<EntryPoint>>, symbol: SymbolKey) -> PythonArchBuilder {
        PythonArchBuilder {
            entry_point: entry_point,
            file: symbol, //dummy, evaluated in load_arch
            file_mode: false, //dummy, evaluated in load_arch
            current_step: BuildSteps::ARCH, //dummy, evaluated in load_arch
            sym_stack: vec![symbol],
            __all_symbols_to_add: Vec::new(),
            diagnostics: vec![],
            file_info: None,
        }
    }

    pub fn load_arch(&mut self, session: &mut SessionInfo) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }

        let symbol = self.sym_stack[0];
        if matches!(symbol, SymbolKey::Namespace(_) | SymbolKey::Root(_) | SymbolKey::Compiled(_) | SymbolKey::Variable(_) | SymbolKey::Class(_)) {
            return; // nothing to extract
        }
        let file = st!().get_file(symbol).unwrap();
        self.file = file;
        self.file_mode = file == symbol;
        self.current_step = if self.file_mode {BuildSteps::ARCH} else {BuildSteps::VALIDATION};
        let symbol_view = get_sym!(st!(), symbol);
        if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !symbol_view.is_external()) {
            trace!("building {} - {}", get_sym!(st!(), self.file).paths().first().unwrap_or(&S!("No path found")), symbol_view.name());
        }
        st!().set_build_status(symbol, BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let path = get_sym!(st!(), self.file).get_symbol_first_path();
        if self.file_mode {
            let in_workspace = get_sym!(st!(), self.file).parent()
                .and_then(|p| st!().get_symbol_view(p))
                .is_some_and(|parent_sym| parent_sym.in_workspace()) ||
                SyncOdoo::is_in_workspace_or_entry(session, &path);
            st!().set_in_workspace(self.file, in_workspace);
        }
        if let SymbolKey::Module(m) = symbol  {
            // @arena: is odoo_addons always a namespace?
            let odoo_addons = st!().modules[m].parent;
            // @ arena: borrow conflict here??
            ModuleSymbol::load_module_info(m, session, odoo_addons);
            ModuleSymbol::load_data(m, session);
        }
        let file_info_rc = match self.file_mode {
            true => {
                let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path.as_str(), None, None, false); //create ast if not in cache
                file_info
                },
            false => {session.sync_odoo.get_file_mgr().borrow().get_file_info(&path).unwrap()}
        };
        self.file_info = Some(file_info_rc.clone());
        if self.file_mode {
            //diagnostics for functions are stored directly on funcs
            let mut file_info = file_info_rc.borrow_mut();
            file_info.replace_diagnostics(BuildSteps::ARCH, self.diagnostics.clone());
        }
        if file_info_rc.borrow().file_info_ast.borrow().indexed_module.is_none() {
            file_info_rc.borrow_mut().prepare_ast(session);
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
        if file_info_ast.indexed_module.is_some() {
            let ast = if self.file_mode {
                file_info_ast.get_stmts().unwrap()
            } else {
                // @arena: formely unwrap on Option that was only Some for the function case
                let f = self.sym_stack[0].unwrap_function_key();
                let ast_index = st!()[f].node_index.load();
                if ast_index.as_u32().is_some() {
                    let func = file_info_ast.indexed_module.as_ref().unwrap().get_by_index(ast_index);
                    match func {
                        AnyRootNodeRef::Stmt(Stmt::FunctionDef(func_stmt)) => {
                            &func_stmt.body
                        },
                        _ => panic!("Expected function definition")
                    }
                } else {
                    //if ast_index is empty, this is because the function has been added manually and do not belong to the ast. Skip it's building
                    &vec![]
                }
            };
            let old_stack_noqa = session.noqas_stack.clone();
            session.noqas_stack.clear();
            let old_noqa = if self.file_mode {
                if let Some(file_noqa) = file_noqa {
                    session.noqas_stack.push(file_noqa);
                }
                let new_noqa = combine_noqa_info(&session.noqas_stack);
                st!().set_noqas(symbol, new_noqa.clone()); //only set for file, functions are set in visit_func_def
                let old = session.current_noqa.clone();
                session.current_noqa = new_noqa;
                st!().set_processed_text_hash(symbol, file_info_ast.text_hash);
                old
            } else {
                let noqas = st!().get_noqas(symbol);
                session.noqas_stack.push(noqas.clone());
                let old = session.current_noqa.clone();
                session.current_noqa = noqas;
                old
            };
            let _ = self.visit_node(session, &ast);
            session.current_noqa = old_noqa;
            session.noqas_stack = old_stack_noqa;
            self._resolve_all_symbols(session);
            if self.file_mode {
                session.sync_odoo.add_to_rebuild_arch_eval(self.sym_stack[0]);
            }
        } else if self.file_mode {
            if get_sym!(st!(), symbol).typ() == SymType::PACKAGE(PackageType::MODULE) {
                //even if there is no __init__.py, we need to go to rebuild_arch and validation to validate the manifest
                session.sync_odoo.add_to_rebuild_arch_eval(self.sym_stack[0]);
            } else {
                let mut file_info = file_info_rc.borrow_mut();
                file_info.publish_diagnostics(session);
            }
        }
        PythonArchBuilderHooks::on_done(session, self.sym_stack[0]);
        st!().set_build_status(self.sym_stack[0], BuildSteps::ARCH, BuildStatus::DONE);
    }

    fn create_local_symbols_from_import_stmt(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: u32, _range: &TextRange) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
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
                    st!().not_found_paths_mut(self.file).push((self.current_step, import_result.file_tree.clone()));
                    continue;
                }
                let mut all_name_allowed = true;
                let mut name_filter: Vec<OYarn> = vec![];
                for import_symbol in import_result.symbols {
                    if let Some(all) = st!().get_content_symbol(import_symbol, "__all__", u32::MAX).symbols.first().copied() {
                        let all_value = follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                            all, None, false
                        )), session, &mut None, false, true, None, None);
                        if let Some(all_value_first) = all_value.get(0) {
                            if !st!().is_expired_if_weak(all_value_first) {
                                let all_upgraded = st!().upgrade_weak(all_value_first);
                                if let Some(all_upgraded_unwrapped) = all_upgraded {
                                    let all_upgraded_unwrapped_bw = get_sym!(st!(), all_upgraded_unwrapped);
                                    let evaluations = all_upgraded_unwrapped_bw.evaluations();
                                    if let Some(evals) = evaluations && evals.len() == 1 {
                                        let value = &evals[0].value;
                                        if value.is_some() {
                                            let (nf, parse_error) = self.extract_all_symbol_eval_values(&value.as_ref());
                                            if parse_error {
                                                warn!("error during parsing __all__ import in file {}", get_sym!(st!(), import_symbol).paths()[0] )
                                            }
                                            name_filter = nf;
                                            all_name_allowed = false;
                                        } else {
                                            warn!("invalid __all__ import in file {} - no value found", get_sym!(st!(), import_symbol).paths()[0])
                                        }
                                    } else {
                                        warn!("invalid __all__ import in file {} - multiple evaluation found", get_sym!(st!(), import_symbol).paths()[0])
                                    }
                                } else {
                                    warn!("invalid __all__ import in file {} - localizedSymbol not found", get_sym!(st!(), import_symbol).paths()[0])
                                }
                            } else {
                                warn!("invalid __all__ import in file {} - expired symbol", get_sym!(st!(), import_symbol).paths()[0])
                            }
                        } else {
                            warn!("invalid __all__ import in file {} - no symbol found", get_sym!(st!(), import_symbol).paths()[0])
                        }
                    }
                    if !matches!(import_symbol, SymbolKey::Compiled(_)) {
                        if *self.sym_stack.last().unwrap() != import_symbol { /*We have to check that the imported symbol is not the current one. It can
                            happen for example in a .pyi that is importing the .pyd file with the same name. As both exists, odools will try to import the pyi a second time in the same file,
                            and so create a borrow error here
                            */
                            let mut import_variables_to_create = vec![];
                            for (name, loc_syms) in st!().iter_symbols(import_symbol) {
                                if all_name_allowed || name_filter.contains(&name) {
                                    let evaluations = Evaluation::from_sections(&st!(), import_symbol, loc_syms);
                                    import_variables_to_create.push((name.clone(), evaluations));
                                }
                            }
                            for (name, evaluations) in import_variables_to_create {
                                let evaluated_type = &evaluations[0].symbol;
                                let evaluated_type = evaluated_type.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, None).weak;
                                if let Some(evaluated_type) = evaluated_type.upgrade(&st!()) {
                                    let evaluated_type_file = st!().get_file(evaluated_type).unwrap();
                                    if !(self.file == evaluated_type_file) {
                                        st!().add_dependency(self.file, evaluated_type_file, self.current_step, BuildSteps::ARCH);
                                    }
                                }
                                let variable_key = st!().add_new_variable(*self.sym_stack.last().unwrap(), name, &import_result.range);
                                let variable = &mut st!().variables[variable_key];
                                variable.is_import_variable = true;
                                variable.evaluations = evaluations;
                            }
                        }
                    }
                }
            } else {
                let var_name = if import_name.asname.is_none() {
                    S!(import_name.name.split(".").next().unwrap())
                } else {
                    import_name.asname.as_ref().unwrap().clone().to_string()
                };
                let variable_key = st!().add_new_variable(*self.sym_stack.last().unwrap(), OYarn::from(var_name), &import_name.range);
                st!().variables[variable_key].is_import_variable = true;
            }
        }
        Ok(())
    }

    fn visit_node(&mut self, session: &mut SessionInfo, nodes: &Vec<Stmt>) -> Result<(), Error> {
        for stmt in nodes.iter() {
            match stmt {
                Stmt::Import(import_stmt) => {
                    self.create_local_symbols_from_import_stmt(session, None, &import_stmt.names, 0, &import_stmt.range)?
                },
                Stmt::ImportFrom(import_from_stmt) => {
                    self.create_local_symbols_from_import_stmt(session, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level, &import_from_stmt.range)?
                },
                Stmt::AnnAssign(ann_assign_stmt) => {
                    self._visit_ann_assign(session, ann_assign_stmt);
                },
                Stmt::Assign(assign_stmt) => {
                    self._visit_assign(session, assign_stmt);
                },
                Stmt::FunctionDef(function_def_stmt) => {
                    self.visit_func_def(session, function_def_stmt)?;
                },
                Stmt::ClassDef(class_def_stmt) => {
                    self.visit_class_def(session, class_def_stmt)?;
                },
                Stmt::If(if_stmt) => {
                    self.visit_if(session, if_stmt)?;
                },
                Stmt::Try(try_stmt) => {
                    self.visit_try(session, try_stmt)?;
                },
                Stmt::For(for_stmt) => {
                    self.visit_for(session, for_stmt)?;
                },
                Stmt::With(with_stmt) => {
                    self.visit_with(session, with_stmt)?;
                },
                Stmt::Match(match_stmt) => {
                    self.visit_match(session, match_stmt)?;
                },
                Stmt::While(while_stmt) => {
                    self.visit_while(session, while_stmt)?;
                },
                Stmt::Expr(stmt_expression) => {
                    self.visit_expr(session, &stmt_expression.value);
                },
                Stmt::Return(return_stmt) => {
                    if let Some(value) = return_stmt.value.as_ref() {
                        self.visit_expr(session, &value);
                    }
                },
                Stmt::Assert(assert_stmt) => {
                    self.visit_expr(session, &assert_stmt.test);
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
        Ok(())
    }

    fn visit_expr(&mut self, session: &mut SessionInfo, expr: &Expr){
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        match expr {
            Expr::Named(named_expr) =>{
                self.visit_named_expr(session, &named_expr);
            },
            Expr::BoolOp(bool_op_expr) => {
                // introduce sections here
                // Due to short circuit behavior
                // Further conditions can be skipped
                // Which could have named expressions

                // one section per value
                // one succeeding section with all the value sections in OR
                let scope = *self.sym_stack.last().unwrap();
                let mut prev_section = st!().get_as_symbol_mgr(scope).get_last_index();
                let cond_sections = bool_op_expr.values.iter().map(|expr|{
                    st!().get_as_mut_symbol_mgr(scope).add_section(
                        expr.range().start(),
                        Some(SectionIndex::INDEX(prev_section))
                    ).index;
                    self.visit_expr(session, &expr);
                    prev_section = st!().get_as_symbol_mgr(scope).get_last_index();
                    SectionIndex::INDEX(prev_section)
                }).collect::<Vec<_>>();
                st!().get_as_mut_symbol_mgr(scope).add_section(
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
            Expr::Lambda(_todo_lambda_expr) => {
                // Lambdas can have named expressions, but it is not a common use
                // Like lambda vals: vals[(x := 0): x + 3]
                // However x is only in scope in the lambda expression only
                // It needs adding a new function, ast_indexes, then add the variable inside
                // I deem it currently unnecessary
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
                        match c {
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
            Some(value) => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        for assign in assigns.iter() { //should only be one
            if let Some(ref expr) = assign.value{
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    session.sync_odoo.symbol_table.add_new_variable(*self.sym_stack.last().unwrap(), oyarn!("{}", name_expr.id), &name_expr.range);
                },
                AssignTargetType::Attribute(ref _attr_expr) => {
                }
            }
        }
    }

    fn _visit_assign(&mut self, session: &mut SessionInfo, assign_stmt: &StmtAssign) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        for assign in assigns.iter() {
            if let Some(ref expr) = assign.value {
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    let variable_key = st!().add_new_variable(*self.sym_stack.last().unwrap(), oyarn!("{}", name_expr.id), &name_expr.range);
                    let variable = &st!().variables[variable_key];
                    if self.file_mode && variable.name == "__all__" && assign.value.is_some() {
                        let mut deps = vec![vec![]]; //only arch level
                        let eval = Evaluation::eval_from_ast(session, &assign.value.as_ref().unwrap(), variable.parent, &assign_stmt.range.start(), false, &mut deps);
                        st!().insert_dependencies(self.file, &deps, BuildSteps::ARCH);
                        st!().variables[variable_key].evaluations = eval.0;
                        self.diagnostics.extend(eval.1);
                        if let Some(evaluation) = st!().variables[variable_key].evaluations.get(0) {
                            if get_sym!(st!(), *self.sym_stack.last().unwrap()).is_external() {
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
            session.sync_odoo.symbol_table.add_new_variable(*self.sym_stack.last().unwrap(), oyarn!("{}", name_expr.id), &named_expr.target.range());
        }
    }

    fn visit_func_def(&mut self, session: &mut SessionInfo, func_def: &StmtFunctionDef) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        if func_def.body.is_empty() {
            return Ok(()) //if body is empty, it usually means that the ast of the class is invalid. Skip it
        }
        let function_key = st!().add_new_function(*self.sym_stack.last().unwrap(),
            &func_def.name.id.to_string(), &func_def.range, &func_def.body.get(0).unwrap().range().start());
        let func_sym = &mut st!()[function_key];
        func_sym.node_index.set(func_def.node_index.load());
        for decorator in func_def.decorator_list.iter() {
            if decorator.expression.is_name_expr() {
                if decorator.expression.as_name_expr().unwrap().id.to_string() == "staticmethod" {
                    func_sym.is_static = true;
                }
                else if decorator.expression.as_name_expr().unwrap().id.to_string() == "property" {
                    func_sym.is_property = true;
                }
                else if decorator.expression.as_name_expr().unwrap().id.to_string() == "overload" {
                    func_sym.is_overloaded = true;
                }
                else if decorator.expression.as_name_expr().unwrap().id.to_string() == "classmethod" {
                    func_sym.is_class_method = true;
                }
                else if decorator.expression.as_name_expr().unwrap().id.to_string() == "classproperty" ||
                decorator.expression.as_name_expr().unwrap().id.to_string() == "lazy_classproperty" {
                    func_sym.is_property = true;
                    func_sym.is_class_method = true;
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
        for arg in func_def.parameters.posonlyargs.iter() {
            let param = st!().add_new_variable(function_key, oyarn!("{}", arg.parameter.name.id), &arg.range);
            st!().variables[param].is_parameter = true;
            let mut default = None;
            if arg.default.is_some() {
                default = Some(Evaluation::new_none()); //TODO evaluate default? actually only used to know if there is a default or not
            }
            st!()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: default,
                arg_type: ArgumentType::POS_ONLY,
                annotation: arg.parameter.annotation.clone(),
            });
        }
        for arg in func_def.parameters.args.iter() {
            let param = st!().add_new_variable(function_key, oyarn!("{}", arg.parameter.name.id), &arg.range);
            st!().variables[param].is_parameter = true;
            let mut default = None;
            if arg.default.is_some() {
                default = Some(Evaluation::new_none()); //TODO evaluate default? actually only used to know if there is a default or not
            }
            st!()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: default,
                arg_type: ArgumentType::ARG,
                annotation: arg.parameter.annotation.clone(),
            });
        }
        if let Some(arg) = &func_def.parameters.vararg {
            let param = st!().add_new_variable(function_key, oyarn!("{}", arg.name.id), &arg.range);
            st!().variables[param].is_parameter = true;
            st!()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: None,
                arg_type: ArgumentType::VARARG,
                annotation: arg.annotation.clone(),
            });
        }
        for arg in func_def.parameters.kwonlyargs.iter() {
            let param = st!().add_new_variable(function_key, oyarn!("{}", arg.parameter.name.id), &arg.range);
            st!().variables[param].is_parameter = true;
            st!()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: arg.default.as_ref().map(|_default| Evaluation::new_none()),
                arg_type: ArgumentType::KWORD_ONLY,
                annotation: arg.parameter.annotation.clone(),
            });
        }
        if let Some(arg) = &func_def.parameters.kwarg {
            let param = st!().add_new_variable(function_key, oyarn!("{}", arg.name.id), &arg.range);
            st!().variables[param].is_parameter = true;
            st!()[function_key].args.push(Argument {
                symbol: param.into(),
                default_value: None,
                arg_type: ArgumentType::KWARG,
                annotation: arg.annotation.clone(),
            });
        }
        let mut add_noqa = false;
        if let Some(noqa_bloc) = self.file_info.as_ref().unwrap().borrow().noqas_blocs.get(&func_def.range.start().to_u32()) {
            session.noqas_stack.push(noqa_bloc.clone());
            add_noqa = true;
        }
        let noqa = combine_noqa_info(&session.noqas_stack);
        st!()[function_key].noqas = noqa.clone();
        session.current_noqa = noqa;
        //visit body
        if !self.file_mode || st!().get_in_parents(function_key.into(), &[SymType::CLASS], true).is_none() {
            st!()[function_key].arch_status = BuildStatus::IN_PROGRESS;
            self.sym_stack.push(function_key.into());
            self.visit_node(session, &func_def.body)?;
            self.sym_stack.pop();
            st!()[function_key].arch_status = BuildStatus::DONE;
        }
        if add_noqa {
            session.noqas_stack.pop();
        }
        Ok(())
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, class_def: &StmtClassDef) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        if class_def.body.is_empty() {
            return Ok(()) //if body is empty, it usually means that the ast of the class is invalid. Skip it
        }
        let parent = *self.sym_stack.last().unwrap();
        let class_key = st!().add_new_class(
            parent, &class_def.name.id.to_string(), &class_def.range, &class_def.body.get(0).unwrap().range().start());
        let class_sym = &mut st!().classes[class_key];

        if class_def.body.len() > 0 && class_def.body[0].is_expr_stmt() {
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
        self.visit_node(session, &class_def.body)?;
        self.sym_stack.pop();
        if add_noqa {
            session.noqas_stack.pop();
        }
        PythonArchBuilderHooks::on_class_def(session, &self.entry_point, class_key);
        Ok(())
    }

    fn _resolve_all_symbols(&mut self, session: &mut SessionInfo) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let parent = *self.sym_stack.last().unwrap();
        for (symbol_name, range) in self.__all_symbols_to_add.drain(..) {
            if st!().get_content_symbol(parent, &symbol_name, u32::MAX).symbols.is_empty() {
                st!().add_new_variable(parent, oyarn!("{}", symbol_name), &range);
            }
        }
    }

    fn check_tuples(&self, version: &Vec<u32>, op: &CmpOp, tuple: &ExprTuple) -> bool {
        let mut tuple = tuple.elts.iter().map(|elt| {
            if let Expr::NumberLiteral(num) = elt {
                if num.value.is_int() {
                    num.value.as_int().unwrap().as_u32().unwrap()
                } else {
                    0 as u32
                }
            } else {
                0 as u32 // If not a number, treat as 0
            }
        }).collect::<Vec<u32>>();
        // ensure that the vec is sized of 3
        tuple.resize(3, 0);
        return match op {
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
        if let Expr::Compare(expr_comp) = expr {
            if expr_comp.comparators.len() == 1 {
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
                if attr.value.is_name_expr() && attr.value.as_name_expr().unwrap().id == "sys" {
                    if attr.attr.id == "version_info" {
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
            }
        }
        (true, false)
    }

    fn visit_if(&mut self, session: &mut SessionInfo, if_stmt: &StmtIf) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        //TODO check platform condition (sys.version > 3.12, etc...)
        let scope = *self.sym_stack.last().unwrap();
        let scope_as_sym_mgr = st!().get_as_mut_symbol_mgr(scope);
        let prefix_section = scope_as_sym_mgr.get_last_index();
        let test_section = scope_as_sym_mgr.add_section(
            if_stmt.test.range().start(),
            None // Take preceding section (before if stmt)
        );
        let mut last_test_section = test_section.index;

        self.visit_expr(session, &if_stmt.test);
        let mut body_version_ok = false; //if true, it means we found a condition that is true and contained a version check. Used to avoid else clause
        let mut stmt_sections = if if_stmt.body.is_empty() {
            vec![]
        } else {
                st!().get_as_mut_symbol_mgr(scope).add_section( // first body section
                    if_stmt.body[0].range().start(),
                    None // Take preceding section (if test)
                );
            let check_version = self._check_sys_version_condition(session, if_stmt.test.as_ref());
            if check_version.0 {
                if check_version.1 {
                    body_version_ok = true;
                }
                self.visit_node(session, &if_stmt.body)?;
                vec![ SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index())]
            } else {
                vec![]
            }
        };

        let mut else_clause_exists = false;

        let stmt_clauses_iter = if_stmt.elif_else_clauses.iter().map(|elif_else_clause|{
            match elif_else_clause.test {
                Some(ref test_clause) => {
                    last_test_section = st!().get_as_mut_symbol_mgr(scope).add_section(
                        test_clause.range().start(),
                        Some(SectionIndex::INDEX(last_test_section))
                    ).index;
                    self.visit_expr(session, test_clause);
                },
                None => else_clause_exists = true
            }
            if elif_else_clause.body.is_empty() {
                return Ok::<Option<SectionIndex>, Error>(None);
            }
            st!().get_as_mut_symbol_mgr(scope).add_section(
                elif_else_clause.body[0].range().start(),
                Some(SectionIndex::INDEX(last_test_section))
            );
            if elif_else_clause.test.is_some() {
                let version_check = self._check_sys_version_condition(session, elif_else_clause.test.as_ref().unwrap());
                if version_check.0 {
                    if version_check.1 {
                        body_version_ok = true;
                    }
                    self.visit_node(session, &elif_else_clause.body)?;
                }
            }
            else if !body_version_ok { //else clause
                self.visit_node(session, &elif_else_clause.body)?;
            }
            let clause_section = SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index());
            Ok::<Option<SectionIndex>, Error>(Some(clause_section))
        });

        stmt_sections.extend(stmt_clauses_iter.collect::<Result<Vec<_>, _>>()?.into_iter().filter_map(|x| x).collect::<Vec<_>>());

        if !else_clause_exists{
            // If there is no else clause, the there is an implicit else clause
            // Which bypasses directly to the last test section
            stmt_sections.push(SectionIndex::INDEX(last_test_section));
        }
        if stmt_sections.is_empty(){
            // If there are no valid bodies or tests, point to the section before the if-stmt
            stmt_sections.push(SectionIndex::INDEX(prefix_section));
        }
        st!().get_as_mut_symbol_mgr(scope).add_section(
            if_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }

    fn visit_for(&mut self, session: &mut SessionInfo, for_stmt: &StmtFor) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        // TODO: Handle breaks for sections
        let scope = *self.sym_stack.last().unwrap();
        let unpacked = python_utils::unpack_assign(&vec![*for_stmt.target.clone()], None, None);
        for assign in unpacked {
            if let Some(ref expr) = assign.value {
                self.visit_expr(session, expr);
            }
            match assign.target {
                AssignTargetType::Name(ref name_expr) => {
                    st!().add_new_variable(scope, oyarn!("{}", name_expr.id), &name_expr.range);
                },
                AssignTargetType::Attribute(_) => {
                }
            }
        }
        let scope_as_sym_mgr = st!().get_as_mut_symbol_mgr(scope);
        let previous_section = SectionIndex::INDEX(scope_as_sym_mgr.get_last_index());
        if let Some(first_body_stmt) = for_stmt.body.first() {
            scope_as_sym_mgr.add_section(
                first_body_stmt.range().start(),
                None
            );
        }

        self.visit_node(session, &for_stmt.body)?;
        let mut stmt_sections = vec![SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index())];

        if !for_stmt.orelse.is_empty(){
            st!().get_as_mut_symbol_mgr(scope).add_section(
                for_stmt.orelse[0].range().start(),
                Some(previous_section.clone())
            );
            self.visit_node(session, &for_stmt.orelse)?;
            stmt_sections.push(SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index()));
        } else {
            stmt_sections.push(previous_section.clone());
        }

        st!().get_as_mut_symbol_mgr(scope).add_section(
            for_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }

    fn visit_try(&mut self, session: &mut SessionInfo, try_stmt: &StmtTry) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        // Try sections:
        // try block is always executed, so it has the same section as the one preceding it.
        // Finally is always executed if it exists, so it belongs to the lower section
        let scope = *self.sym_stack.last().unwrap();
        self.visit_node(session, &try_stmt.body)?;
        if !try_stmt.handlers.is_empty(){
            // Branching around except _T, except, and else act similar to if-elif-else
            // The direct link (eq. to empty section) to previous scope is always there
            // Unless both catch-all except and else clauses exist.
            let previous_section = SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index());
            let mut stmt_sections = vec![previous_section.clone()];
            let mut catch_all_except_exists = false;
            for handler in try_stmt.handlers.iter() {
                match handler {
                    ruff_python_ast::ExceptHandler::ExceptHandler(h) => {
                        if !catch_all_except_exists { catch_all_except_exists = h.type_.is_none()};
                        if h.body.is_empty() {
                            continue;
                        }
                        st!().get_as_mut_symbol_mgr(scope).add_section(
                            h.body[0].range().start(),
                            Some(previous_section.clone())
                        );
                        self.visit_node(session, &h.body)?;
                        stmt_sections.push(SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index()));
                    }
                }
            }
            if !try_stmt.orelse.is_empty(){
                if catch_all_except_exists{
                    stmt_sections.remove(0);
                }
                st!().get_as_mut_symbol_mgr(scope).add_section(
                    try_stmt.orelse[0].range().start(),
                    Some(previous_section.clone())
                );
                self.visit_node(session, &try_stmt.orelse)?;
                stmt_sections.push(SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index()));
            }
            // Next section is either the start of the finally block, or right after the try block if finally does not exist
            let next_section_start = try_stmt.finalbody.first().map(|stmt| stmt.range().start()).unwrap_or(try_stmt.range().end() + TextSize::new(1));
            st!().get_as_mut_symbol_mgr(scope).add_section(
                next_section_start,
                Some(SectionIndex::OR(stmt_sections))
            );
        }
        self.visit_node(session, &try_stmt.finalbody)?;
        Ok(())
    }

    fn visit_with(&mut self, session: &mut SessionInfo, with_stmt: &StmtWith) -> Result<(), Error> {
        for item in with_stmt.items.iter() {
            self.visit_expr(session, &item.context_expr);
            if let Some(var) = item.optional_vars.as_ref() {
                match &**var {
                    Expr::Name(expr_name) => {
                        let parent = *self.sym_stack.last().unwrap();
                        session.sync_odoo.symbol_table.add_new_variable(parent, oyarn!("{}", expr_name.id), &var.range());
                    },
                    Expr::Tuple(_) => {continue;},
                    Expr::List(_) => {continue;},
                    _ => {continue;}
                }
            }
        }
        self.visit_node(session, &with_stmt.body)?;
        Ok(())
    }

    fn visit_match(&mut self, session: &mut SessionInfo, match_stmt: &StmtMatch) -> Result<(), Error> {
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
                            scope, oyarn!("{}", name), &pattern_match_star.range());
                    }
                },
                Pattern::MatchAs(pattern_match_as) => {
                    if let Some(name) = &pattern_match_as.name { //if name is None, this is a wildcard pattern (_)
                        symbol_table.add_new_variable(
                            scope, oyarn!("{}", name), &pattern_match_as.range());
                    }
                },
                Pattern::MatchOr(match_or) => {
                    match_or.patterns.iter().for_each(|pattern| traverse_match(pattern, symbol_table, scope));
                },
            }
        }
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let scope = *self.sym_stack.last().unwrap();
        let previous_section = SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index());
        let mut stmt_sections = vec![previous_section.clone()];
        for case in match_stmt.cases.iter() {
            case.guard.as_ref().map(|test_clause| self.visit_expr(session, test_clause));
            if matches!(&case.pattern, ruff_python_ast::Pattern::MatchAs(_)){
                stmt_sections.remove(0); // When we have a wildcard pattern, previous section is shadowed
            }
            st!().get_as_mut_symbol_mgr(scope).add_section(
                case.range().start(),
                Some(previous_section.clone())
            );
            traverse_match(&case.pattern, &mut st!(), scope);
            self.visit_node(session, &case.body)?;
            stmt_sections.push(SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index()));
        }
        st!().get_as_mut_symbol_mgr(scope).add_section(
            match_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }

    fn visit_while(&mut self, session: &mut SessionInfo, while_stmt: &StmtWhile) -> Result<(), Error> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        // TODO: Handle breaks for sections
        let scope = *self.sym_stack.last().unwrap();
        let scope_as_sym_mgr = st!().get_as_mut_symbol_mgr(scope);
        let previous_section = SectionIndex::INDEX(scope_as_sym_mgr.get_last_index());
        if let Some(first_body_stmt) = while_stmt.body.first() {
            scope_as_sym_mgr.add_section(
                first_body_stmt.range().start(),
                None
            );
        }
        self.visit_expr(session, &while_stmt.test);
        self.visit_node(session, &while_stmt.body)?;
        let scope_as_sym_mgr = st!().get_as_mut_symbol_mgr(scope);
        let body_section = SectionIndex::INDEX(scope_as_sym_mgr.get_last_index());
        let mut stmt_sections = vec![body_section];
        if !while_stmt.orelse.is_empty(){
            scope_as_sym_mgr.add_section(
                while_stmt.orelse[0].range().start(),
                Some(previous_section.clone())
            );
            self.visit_node(session, &while_stmt.orelse)?;
            stmt_sections.push(SectionIndex::INDEX(st!().get_as_symbol_mgr(scope).get_last_index()));
        } else {
            stmt_sections.push(previous_section.clone());
        }

        st!().get_as_mut_symbol_mgr(scope).add_section(
            while_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }
}
