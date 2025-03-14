use std::rc::Rc;
use std::cell::RefCell;
use std::vec;
use anyhow::Error;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ruff_python_ast::{Alias, Expr, Identifier, Pattern, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFor, StmtFunctionDef, StmtIf, StmtMatch, StmtTry, StmtWhile, StmtWith};
use lsp_types::Diagnostic;
use tracing::{trace, warn};
use weak_table::traits::WeakElement;

use crate::constants::{BuildStatus, BuildSteps, SymType, DEBUG_STEPS};
use crate::core::python_utils;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::symbols::symbol::Symbol;
use crate::core::evaluation::{Evaluation, EvaluationValue};
use crate::core::python_arch_builder_hooks::PythonArchBuilderHooks;
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{EvaluationSymbolPtr, EvaluationSymbolWeak};
use super::import_resolver::ImportResult;
use super::odoo::SyncOdoo;
use super::symbols::function_symbol::{Argument, ArgumentType};
use super::symbols::symbol_mgr::SectionIndex;


#[derive(Debug)]
pub struct PythonArchBuilder {
    entry_point: Rc<RefCell<EntryPoint>>,
    file: Rc<RefCell<Symbol>>,
    file_mode: bool,
    current_step: BuildSteps,
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    __all_symbols_to_add: Vec<(String, TextRange)>,
    diagnostics: Vec<Diagnostic>
}

impl PythonArchBuilder {
    pub fn new(entry_point: Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) -> PythonArchBuilder {
        PythonArchBuilder {
            entry_point: entry_point,
            file: symbol.clone(), //dummy, evaluated in load_arch
            file_mode: false, //dummy, evaluated in load_arch
            current_step: BuildSteps::ARCH, //dummy, evaluated in load_arch
            sym_stack: vec![symbol],
            __all_symbols_to_add: Vec::new(),
            diagnostics: vec![]
        }
    }

    pub fn load_arch(&mut self, session: &mut SessionInfo) {
        let symbol = &self.sym_stack[0];
        if [SymType::NAMESPACE, SymType::ROOT, SymType::COMPILED, SymType::VARIABLE, SymType::CLASS].contains(&symbol.borrow().typ()) {
            return; // nothing to extract
        }
        {
            let file = symbol.borrow();
            let file = file.get_file().unwrap();
            let file = file.upgrade().unwrap();
            self.file = file.clone();
            self.file_mode = Rc::ptr_eq(&file, &symbol);
            self.current_step = if self.file_mode {BuildSteps::ARCH} else {BuildSteps::VALIDATION};
        }
        if DEBUG_STEPS {
            trace!("building {} - {}", self.file.borrow().paths().first().unwrap_or(&S!("No path found")), symbol.borrow().name());
        }
        symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let path = self.file.borrow().get_symbol_first_path();
        if self.file_mode {
            let in_workspace = (self.file.borrow().parent().is_some() &&
                self.file.borrow().parent().as_ref().unwrap().upgrade().is_some() &&
                self.file.borrow().parent().as_ref().unwrap().upgrade().unwrap().borrow().in_workspace()) ||
                SyncOdoo::is_in_workspace_or_entry(session, path.as_str());
            self.file.borrow_mut().set_in_workspace(in_workspace);
        }
        let file_info_rc = match self.file_mode {
            true => {
                let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path.as_str(), None, None, false); //create ast if not in cache
                file_info
                },
            false => {session.sync_odoo.get_file_mgr().borrow().get_file_info(&path).unwrap()}
        };
        if !file_info_rc.borrow().valid {
            symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::PENDING);
            return
        }
        if self.file_mode {
            //diagnostics for functions are stored directly on funcs
            let mut file_info = file_info_rc.borrow_mut();
            file_info.replace_diagnostics(BuildSteps::ARCH, self.diagnostics.clone());
        }
        let file_info = file_info_rc.borrow();
        if file_info.ast.is_some() {
            let ast = match self.file_mode {
                true => {file_info.ast.as_ref().unwrap()},
                false => {
                    &AstUtils::find_stmt_from_ast(file_info.ast.as_ref().unwrap(), self.sym_stack[0].borrow().ast_indexes().unwrap()).as_function_def_stmt().unwrap().body
                }
            };
            if self.file_mode {
                symbol.borrow_mut().set_processed_text_hash(file_info.text_hash);
            }
            self.visit_node(session, &ast);
            self._resolve_all_symbols(session);
            if self.file_mode {
                session.sync_odoo.add_to_rebuild_arch_eval(self.sym_stack[0].clone());
            }
        } else if self.file_mode {
            drop(file_info);
            let mut file_info = file_info_rc.borrow_mut();
            file_info.publish_diagnostics(session);
        }
        PythonArchBuilderHooks::on_done(session, &self.sym_stack[0]);
        let mut symbol = self.sym_stack[0].borrow_mut();
        symbol.set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
    }

    fn create_local_symbols_from_import_stmt(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) -> Result<(), Error> {
        for import_name in name_aliases {
            if import_name.name.as_str() == "*" {
                if self.sym_stack.len() != 1 { //only at top level for now.
                    continue;
                }
                let import_result: ImportResult = resolve_import_stmt(
                    session,
                    self.sym_stack.last().unwrap(),
                    from_stmt,
                    name_aliases,
                    level,
                    &mut None).remove(0); //we don't need the vector with this call as there will be 1 result.
                if !import_result.found {
                    self.entry_point.borrow_mut().not_found_symbols.insert(self.file.clone());
                    let file_tree_flattened = [import_result.file_tree.0.clone(), import_result.file_tree.1.clone()].concat();
                    self.file.borrow_mut().not_found_paths_mut().push((self.current_step, file_tree_flattened));
                    continue;
                }
                let mut all_name_allowed = true;
                let mut name_filter: Vec<String> = vec![];
                if let Some(all) = import_result.symbol.borrow().get_content_symbol("__all__", u32::MAX).get(0) {
                    let all = Symbol::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                        Rc::downgrade(all), None, false
                    )), session, &mut None, false, true, None, &mut self.diagnostics);
                    if let Some(all) = all.get(0) {
                        if !all.is_expired_if_weak() {
                            let all = all.upgrade_weak();
                            if let Some(all) = all {
                                let all = (*all).borrow();
                                if all.evaluations().is_some() && all.evaluations().unwrap().len() == 1 {
                                    let value = &all.evaluations().unwrap()[0].value;
                                    if value.is_some() {
                                        let (nf, parse_error) = self.extract_all_symbol_eval_values(&value.as_ref());
                                        if parse_error {
                                            warn!("error during parsing __all__ import in file {}", (*import_result.symbol).borrow().paths()[0] )
                                        }
                                        name_filter = nf;
                                        all_name_allowed = false;
                                    } else {
                                        warn!("invalid __all__ import in file {} - no value found", (*import_result.symbol).borrow().paths()[0])
                                    }
                                } else {
                                    warn!("invalid __all__ import in file {} - multiple evaluation found", (*import_result.symbol).borrow().paths()[0])
                                }
                            } else {
                                warn!("invalid __all__ import in file {} - localizedSymbol not found", (*import_result.symbol).borrow().paths()[0])
                            }
                        } else {
                            warn!("invalid __all__ import in file {} - expired symbol", (*import_result.symbol).borrow().paths()[0])
                        }
                    } else {
                        warn!("invalid __all__ import in file {} - no symbol found", (*import_result.symbol).borrow().paths()[0])
                    }
                }
                let mut dep_to_add = vec![];
                let symbol = import_result.symbol.borrow();
                if symbol.typ() != SymType::COMPILED {
                    for (name, loc_syms) in symbol.iter_symbols() {
                        if all_name_allowed || name_filter.contains(&name) {
                            let variable = self.sym_stack.last().unwrap().borrow_mut().add_new_variable(session, &name, &import_result.range);
                            let mut loc = variable.borrow_mut();
                            loc.as_variable_mut().is_import_variable = true;
                            loc.as_variable_mut().evaluations = Evaluation::from_sections(&symbol, loc_syms);
                            dep_to_add.push(variable.clone());
                        }
                    }
                }
                drop(symbol);
                for sym in dep_to_add {
                    let mut sym_bw = sym.borrow_mut();
                    let evaluation = &sym_bw.as_variable_mut().evaluations[0];
                    let evaluated_type = &evaluation.symbol;
                    let evaluated_type = evaluated_type.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, None).weak;
                    if !evaluated_type.is_expired() {
                        let evaluated_type = evaluated_type.upgrade().unwrap();
                        let evaluated_type_file = evaluated_type.borrow_mut().get_file().unwrap().clone().upgrade().unwrap();
                        if !Rc::ptr_eq(&self.file, &evaluated_type_file) {
                            self.file.borrow_mut().add_dependency(&mut evaluated_type_file.borrow_mut(), self.current_step, BuildSteps::ARCH);
                        }
                    }
                }

            } else {
                let var_name = if import_name.asname.is_none() {
                    S!(import_name.name.split(".").next().unwrap())
                } else {
                    import_name.asname.as_ref().unwrap().clone().to_string()
                };
                let mut variable = self.sym_stack.last().unwrap().borrow_mut().add_new_variable(session, &var_name, &import_name.range);
                variable.borrow_mut().as_variable_mut().is_import_variable = true;
            }
        }
        Ok(())
    }

    fn visit_node(&mut self, session: &mut SessionInfo, nodes: &Vec<Stmt>) -> Result<(), Error> {
        for stmt in nodes.iter() {
            match stmt {
                Stmt::Import(import_stmt) => {
                    self.create_local_symbols_from_import_stmt(session, None, &import_stmt.names, None, &import_stmt.range)?
                },
                Stmt::ImportFrom(import_from_stmt) => {
                    self.create_local_symbols_from_import_stmt(session, import_from_stmt.module.as_ref(), &import_from_stmt.names, Some(import_from_stmt.level), &import_from_stmt.range)?
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
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn extract_all_symbol_eval_values(&self, value: &Option<&EvaluationValue>) -> (Vec<String>, bool) {
        let mut parse_error = false;
        let vec: Vec<String> = match value {
            Some(eval) => {
                match eval {
                    EvaluationValue::ANY() => {
                        parse_error = true;
                        vec![]
                    }
                    EvaluationValue::CONSTANT(c) => {
                        match c {
                            Expr::StringLiteral(s) => {
                                vec![s.value.to_string()]
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
                                    res.push(s.value.to_string());
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
                                    res.push(s.value.to_string());
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
            self.sym_stack.last().unwrap().borrow_mut().add_new_variable(session, &assign.target.id.to_string(), &assign.target.range);
        }
    }

    fn _visit_assign(&mut self, session: &mut SessionInfo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        for assign in assigns.iter() {
            let variable = self.sym_stack.last().unwrap().borrow_mut().add_new_variable(session, &assign.target.id.to_string(), &assign.target.range);
            let mut variable = variable.borrow_mut();
            if self.file_mode && variable.name() == "__all__" && assign.value.is_some() && variable.parent().is_some() {
                let parent = variable.parent().as_ref().unwrap().upgrade();
                if parent.is_some() {
                    let parent = parent.unwrap();
                    let eval = Evaluation::eval_from_ast(session, &assign.value.as_ref().unwrap(), parent, &assign_stmt.range.start());
                    variable.as_variable_mut().evaluations = eval.0;
                    self.diagnostics.extend(eval.1);
                    if !variable.as_variable().evaluations.is_empty() {
                        if (*self.sym_stack.last().unwrap()).borrow().is_external() {
                            // external packages often import symbols from compiled files
                            // or with meta programmation like globals["var"] = __get_func().
                            // we don't want to handle that, so just declare __all__ content
                            // as symbols to not raise any error.
                            let evaluation = variable.as_variable_mut().evaluations.get(0).unwrap();
                            match &evaluation.value {
                                Some(EvaluationValue::LIST(list)) => {
                                    for item in list.iter() {
                                        match item {
                                            Expr::StringLiteral(s) => {
                                                self.__all_symbols_to_add.push((s.value.to_string(), evaluation.range.unwrap()));
                                            },
                                            _ => {}
                                        }
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    fn visit_func_def(&mut self, session: &mut SessionInfo, func_def: &StmtFunctionDef) -> Result<(), Error> {
        let sym = self.sym_stack.last().unwrap().borrow_mut().add_new_function(
            session, &func_def.name.id.to_string(), &func_def.range, &func_def.body.get(0).unwrap().range().start());
        let mut sym_bw = sym.borrow_mut();
        let func_sym = sym_bw.as_func_mut();
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
            }
        }
        if func_def.body.len() > 0 && func_def.body[0].is_expr_stmt() {
            let expr: &ruff_python_ast::StmtExpr = func_def.body[0].as_expr_stmt().unwrap();
            if let Some(s) = expr.value.as_string_literal_expr() {
                func_sym.doc_string = Some(s.value.to_string())
            }
        }
        drop(sym_bw);
        //add params
        for arg in func_def.parameters.posonlyargs.iter() {
            let param = sym.borrow_mut().add_new_variable(session, &arg.parameter.name.id.to_string(), &arg.range);
            param.borrow_mut().as_variable_mut().is_parameter = true;
            sym.borrow_mut().as_func_mut().args.push(Argument {
                symbol: Rc::downgrade(&param),
                default_value: None,
                arg_type: ArgumentType::POS_ONLY
            });
        }
        for arg in func_def.parameters.args.iter() {
            let param = sym.borrow_mut().add_new_variable(session, &arg.parameter.name.id.to_string(), &arg.range);
            param.borrow_mut().as_variable_mut().is_parameter = true;
            let mut default = None;
            if arg.default.is_some() {
                default = Some(Evaluation::new_none()); //TODO evaluate default? actually only used to know if there is a default or not
            }
            sym.borrow_mut().as_func_mut().args.push(Argument {
                symbol: Rc::downgrade(&param),
                default_value: default,
                arg_type: ArgumentType::ARG
            });
        }
        if let Some(arg) = &func_def.parameters.vararg {
            let param = sym.borrow_mut().add_new_variable(session, &arg.name.id.to_string(), &arg.range);
            param.borrow_mut().as_variable_mut().is_parameter = true;
            sym.borrow_mut().as_func_mut().args.push(Argument {
                symbol: Rc::downgrade(&param),
                default_value: None,
                arg_type: ArgumentType::VARARG
            });
        }
        for arg in func_def.parameters.kwonlyargs.iter() {
            let param = sym.borrow_mut().add_new_variable(session, &arg.parameter.name.id.to_string(), &arg.range);
            param.borrow_mut().as_variable_mut().is_parameter = true;
            sym.borrow_mut().as_func_mut().args.push(Argument {
                symbol: Rc::downgrade(&param),
                default_value: None,
                arg_type: ArgumentType::KWORD_ONLY
            });
        }
        if let Some(arg) = &func_def.parameters.kwarg {
            let param = sym.borrow_mut().add_new_variable(session, &arg.name.id.to_string(), &arg.range);
            param.borrow_mut().as_variable_mut().is_parameter = true;
            sym.borrow_mut().as_func_mut().args.push(Argument {
                symbol: Rc::downgrade(&param),
                default_value: None,
                arg_type: ArgumentType::KWARG
            });
        }
        //visit body
        if !self.file_mode || sym.borrow().get_in_parents(&vec![SymType::CLASS], true).is_none() {
            sym.borrow_mut().as_func_mut().arch_status = BuildStatus::IN_PROGRESS;
            self.sym_stack.push(sym.clone());
            self.visit_node(session, &func_def.body)?;
            self.sym_stack.pop();
            sym.borrow_mut().as_func_mut().arch_status = BuildStatus::DONE;
        }
        Ok(())
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, class_def: &StmtClassDef) -> Result<(), Error> {
        let mut sym = self.sym_stack.last().unwrap().borrow_mut().add_new_class(
            session, &class_def.name.id.to_string(), &class_def.range, &class_def.body.get(0).unwrap().range().start());
        let mut sym_bw = sym.borrow_mut();
        let class_sym = sym_bw.as_class_sym_mut();
        if class_def.body.len() > 0 && class_def.body[0].is_expr_stmt() {
            let expr = class_def.body[0].as_expr_stmt().unwrap();
            if expr.value.is_literal_expr() {
                let const_expr = expr.value.as_literal_expr().unwrap();
                if let Some(s) = const_expr.as_string_literal() {
                    class_sym.doc_string = Some(s.value.to_string());
                }
            }
        }
        drop(sym_bw);
        self.sym_stack.push(sym.clone());
        self.visit_node(session, &class_def.body)?;
        self.sym_stack.pop();
        PythonArchBuilderHooks::on_class_def(session, sym);
        Ok(())
    }

    fn _resolve_all_symbols(&mut self, session: &mut SessionInfo) {
        for (symbol_name, range) in self.__all_symbols_to_add.drain(..) {
            if self.sym_stack.last().unwrap().borrow().get_content_symbol(&symbol_name, u32::MAX).is_empty() {
                let all_var = self.sym_stack.last().unwrap().borrow_mut().add_new_variable(session, &symbol_name, &range);
            }
        }
    }

    fn visit_if(&mut self, session: &mut SessionInfo, if_stmt: &StmtIf) -> Result<(), Error> {
        //TODO check platform condition (sys.version > 3.12, etc...)
        let scope = self.sym_stack.last().unwrap().clone();
        let body_section = scope.borrow_mut().as_mut_symbol_mgr().add_section(
            if_stmt.body.first().unwrap().range().start(),
            None
        );
        let previous_section = SectionIndex::INDEX(body_section.index - 1);
        self.visit_node(session, &if_stmt.body)?;

        let body_section = SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index());

        let mut stmt_sections = vec![body_section];
        let mut else_clause_exists = false;

        stmt_sections.extend(if_stmt.elif_else_clauses.iter().map(|elif_else_clause|{
            scope.borrow_mut().as_mut_symbol_mgr().add_section(
                elif_else_clause.body.first().unwrap().range().start(),
                Some(previous_section.clone())
            );
            if elif_else_clause.test.is_none(){
                else_clause_exists = true;
            }
            self.visit_node(session, &elif_else_clause.body)?;
            let clause_section = SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index());
            Ok::<SectionIndex, Error>(clause_section)
        }).collect::<Result<Vec<_>, _>>()?);

        if !else_clause_exists{
            // If there is no else clause, the there is an implicit else clause
            // Which bypasses directly to the previous_section
            stmt_sections.push(previous_section);
        }
        scope.borrow_mut().as_mut_symbol_mgr().add_section(
            if_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }

    fn visit_for(&mut self, session: &mut SessionInfo, for_stmt: &StmtFor) -> Result<(), Error> {
        // TODO: Handle breaks for sections
        let scope = self.sym_stack.last().unwrap().clone();
        let unpacked = python_utils::unpack_assign(&vec![*for_stmt.target.clone()], None, None);
        for assign in unpacked {
            scope.borrow_mut().add_new_variable(session, &assign.target.id.to_string(), &assign.target.range);
        }
        let previous_section = SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index());
        scope.borrow_mut().as_mut_symbol_mgr().add_section(
            for_stmt.body.first().unwrap().range().start(),
            None
        );

        self.visit_node(session, &for_stmt.body)?;
        let mut stmt_sections = vec![SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index())];

        if !for_stmt.orelse.is_empty(){
            scope.borrow_mut().as_mut_symbol_mgr().add_section(
                for_stmt.orelse.first().unwrap().range().start(),
                Some(previous_section.clone())
            );
            self.visit_node(session, &for_stmt.orelse)?;
            stmt_sections.push(SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index()));
        } else {
            stmt_sections.push(previous_section.clone());
        }

        scope.borrow_mut().as_mut_symbol_mgr().add_section(
            for_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }

    fn visit_try(&mut self, session: &mut SessionInfo, try_stmt: &StmtTry) -> Result<(), Error> {
        // Try sections:
        // try block is always executed, so it has the same section as the one preceding it.
        // Finally is always executed if it exists, so it belongs to the lower section
        let scope = self.sym_stack.last().unwrap().clone();
        self.visit_node(session, &try_stmt.body)?;
        if !try_stmt.handlers.is_empty(){
            // Branching around except _T, except, and else act similar to if-elif-else
            // The direct link (eq. to empty section) to previous scope is always there
            // Unless both catch-all except and else clauses exist.
            let previous_section = SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index());
            let mut stmt_sections = vec![previous_section.clone()];
            let mut catch_all_except_exists = false;
            for handler in try_stmt.handlers.iter() {
                match handler {
                    ruff_python_ast::ExceptHandler::ExceptHandler(h) => {
                        if !catch_all_except_exists { catch_all_except_exists = h.type_.is_none()};
                        scope.borrow_mut().as_mut_symbol_mgr().add_section(
                            h.body.first().unwrap().range().start(),
                            Some(previous_section.clone())
                        );
                        self.visit_node(session, &h.body)?;
                        stmt_sections.push(SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index()));
                    }
                }
            }
            if !try_stmt.orelse.is_empty(){
                if catch_all_except_exists{
                    stmt_sections.remove(0);
                }
                scope.borrow_mut().as_mut_symbol_mgr().add_section(
                    try_stmt.orelse.first().unwrap().range().start(),
                    Some(previous_section.clone())
                );
                self.visit_node(session, &try_stmt.orelse)?;
                stmt_sections.push(SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index()));
            }
            // Next section is either the start of the finally block, or right after the try block if finally does not exist
            let next_section_start = try_stmt.finalbody.first().map(|stmt| stmt.range().start()).unwrap_or(try_stmt.range().end() + TextSize::new(1));
            scope.borrow_mut().as_mut_symbol_mgr().add_section(
                next_section_start,
                Some(SectionIndex::OR(stmt_sections))
            );
        }
        self.visit_node(session, &try_stmt.finalbody)?;
        Ok(())
    }

    fn visit_with(&mut self, session: &mut SessionInfo, with_stmt: &StmtWith) -> Result<(), Error> {
        for item in with_stmt.items.iter() {
            if let Some(var) = item.optional_vars.as_ref() {
                match &**var {
                    Expr::Name(expr_name) => {
                        self.sym_stack.last().unwrap().borrow_mut().add_new_variable(
                            session, &expr_name.id.to_string(), &var.range());
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
        fn traverse_match(pattern: &Pattern, session: &mut SessionInfo, scope: &Rc<RefCell<Symbol>>){
            match pattern {
                Pattern::MatchValue(_) => {},
                Pattern::MatchSingleton(_) => {},
                Pattern::MatchSequence(match_sequence) => {
                    match_sequence.patterns.iter().for_each(|sequence_pattern| traverse_match(sequence_pattern, session, scope));
                },
                Pattern::MatchMapping(match_mapping) => {
                    match_mapping.patterns.iter().for_each(|mapping_value_pattern| traverse_match(mapping_value_pattern, session, scope));
                },
                Pattern::MatchClass(match_class) => {
                    match_class.arguments.patterns.iter().for_each(|class_arg_pattern| traverse_match(class_arg_pattern, session, scope));
                },
                Pattern::MatchStar(pattern_match_star) => {
                    if let Some(name) = &pattern_match_star.name { //if name is None, this is a wildcard pattern (*_)
                        scope.borrow_mut().add_new_variable(
                            session, &name.to_string(), &pattern_match_star.range());
                    }
                },
                Pattern::MatchAs(pattern_match_as) => {
                    if let Some(name) = &pattern_match_as.name { //if name is None, this is a wildcard pattern (_)
                        scope.borrow_mut().add_new_variable(
                            session, &name.to_string(), &pattern_match_as.range());
                    }
                },
                Pattern::MatchOr(match_or) => {
                    match_or.patterns.iter().for_each(|pattern| traverse_match(pattern, session, scope));
                },
            }
        }
        let scope = self.sym_stack.last().unwrap().clone();
        let previous_section = SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index());
        let mut stmt_sections = vec![previous_section.clone()];
        for case in match_stmt.cases.iter() {
            if matches!(&case.pattern, ruff_python_ast::Pattern::MatchAs(_)){
                stmt_sections.remove(0); // When we have a wildcard pattern, previous section is shadowed
            }
            scope.borrow_mut().as_mut_symbol_mgr().add_section(
                case.range().start(),
                Some(previous_section.clone())
            );
            traverse_match(&case.pattern, session, &scope);
            self.visit_node(session, &case.body)?;
            stmt_sections.push(SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index()));
        }
        scope.borrow_mut().as_mut_symbol_mgr().add_section(
            match_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }

    fn visit_while(&mut self, session: &mut SessionInfo, while_stmt: &StmtWhile) -> Result<(), Error> {
        // TODO: Handle breaks for sections
        let scope = self.sym_stack.last().unwrap().clone();
        let body_section = scope.borrow_mut().as_mut_symbol_mgr().add_section(
            while_stmt.body.first().unwrap().range().start(),
            None
        );
        let previous_section = SectionIndex::INDEX(body_section.index - 1);
        self.visit_node(session, &while_stmt.body)?;
        let body_section = SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index());
        let mut stmt_sections = vec![body_section];
        if !while_stmt.orelse.is_empty(){
            scope.borrow_mut().as_mut_symbol_mgr().add_section(
                while_stmt.orelse.first().unwrap().range().start(),
                Some(previous_section.clone())
            );
            self.visit_node(session, &while_stmt.orelse)?;
            stmt_sections.push(SectionIndex::INDEX(scope.borrow().as_symbol_mgr().get_last_index()));
        } else {
            stmt_sections.push(previous_section.clone());
        }

        scope.borrow_mut().as_mut_symbol_mgr().add_section(
            while_stmt.range().end() + TextSize::new(1),
            Some(SectionIndex::OR(stmt_sections))
        );
        Ok(())
    }
}
