use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::{u32, vec};

use ruff_text_size::{Ranged, TextRange, TextSize};
use ruff_python_ast::{Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtExpr, StmtFor, StmtFunctionDef, StmtIf, StmtReturn, StmtTry, StmtWhile, StmtWith};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use tracing::{debug, trace, warn};

use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol::Symbol;
use crate::core::evaluation::Evaluation;
use crate::core::python_utils;
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;
use crate::S;

use super::config::DiagMissingImportsMode;
use super::entry_point::EntryPoint;
use super::evaluation::{ContextValue, EvaluationSymbolPtr, EvaluationSymbolWeak};
use super::file_mgr::FileMgr;
use super::python_arch_eval_hooks::PythonArchEvalHooks;
use super::symbols::function_symbol::FunctionSymbol;
use super::symbols::variable_symbol::{ImportInformation, VariableSymbol};


#[derive(Debug, Clone)]
pub struct PythonArchEval {
    entry_point: Rc<RefCell<EntryPoint>>,
    file: Rc<RefCell<Symbol>>,
    file_mode: bool,
    current_step: BuildSteps,
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    diagnostics: Vec<Diagnostic>,
    safe_import: Vec<bool>,
    ast_indexes: Vec<u16>,
}

impl PythonArchEval {
    pub fn new(entry_point: Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) -> PythonArchEval {
        PythonArchEval {
            entry_point,
            file: symbol.clone(), //dummy, evaluated in eval_arch
            file_mode: false, //dummy, evaluated in eval_arch
            current_step: BuildSteps::ARCH, //dummy, evaluated in eval_arch
            sym_stack: vec![symbol],
            diagnostics: Vec::new(),
            safe_import: vec![false],
            ast_indexes: vec![],
        }
    }

    pub fn eval_arch(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack.first().unwrap().clone();
        if [SymType::NAMESPACE, SymType::ROOT, SymType::COMPILED, SymType::VARIABLE, SymType::CLASS].contains(&symbol.borrow().typ()) {
            return; // nothing to evaluate
        }
        if symbol.borrow().build_status(BuildSteps::ARCH) != BuildStatus::DONE || symbol.borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::PENDING {
            return;
        }
        {
            let file = symbol.borrow();
            let file = file.get_file().unwrap();
            let file = file.upgrade().unwrap();
            self.file = file.clone();
            self.file_mode = Rc::ptr_eq(&file, &symbol);
            self.current_step = if self.file_mode {BuildSteps::ARCH_EVAL} else {BuildSteps::VALIDATION};
            self.ast_indexes = symbol.borrow().ast_indexes().unwrap_or(&vec![]).clone(); //copy current ast_indexes if we are not evaluating a file
        }
        if DEBUG_STEPS {
            trace!("evaluating {} - {}", self.file.borrow().paths().first().unwrap_or(&S!("No path found")), symbol.borrow().name());
        }
        symbol.borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::IN_PROGRESS);
        if self.file.borrow().paths().len() != 1 {
            panic!("Trying to eval_arch a symbol without any path")
        }
        let path = self.file.borrow().get_symbol_first_path();
        let file_info_rc = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path).expect("File not found in cache").clone();
        let file_info = (*file_info_rc).borrow();
        if file_info.ast.is_some() {
            let (ast, func_returns) = match self.file_mode {
                true => {
                    if file_info.text_hash != symbol.borrow().get_processed_text_hash(){
                        symbol.borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::INVALID);
                        return;
                    }
                    (file_info.ast.as_ref().unwrap(), None)
                },
                false => {
                    let func_stmt = AstUtils::find_stmt_from_ast(file_info.ast.as_ref().unwrap(), self.sym_stack[0].borrow().ast_indexes().unwrap()).as_function_def_stmt().unwrap();
                    (&func_stmt.body, func_stmt.returns.as_ref())
                }
            };
            if !self.file_mode {
                PythonArchEval::handle_function_returns(session, func_returns, &self.sym_stack[0], &ast.last().unwrap().range().end(), &mut self.diagnostics);
            }
            for (index, stmt) in ast.iter().enumerate() {
                self.ast_indexes.push(index as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            if !self.file_mode {
                PythonArchEval::handle_func_evaluations(ast, &self.sym_stack[0]);
            }
        }
        drop(file_info);
        if self.file_mode {
            file_info_rc.borrow_mut().replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
            PythonArchEvalHooks::on_file_eval(session, &self.entry_point, self.sym_stack.first().unwrap().clone());
        } else {
            //then Symbol must be a function
            symbol.borrow_mut().as_func_mut().replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
            PythonArchEvalHooks::on_function_eval(session, &self.entry_point, self.sym_stack.first().unwrap().clone());
        }
        let mut symbol = self.sym_stack.first().unwrap().borrow_mut();
        symbol.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::DONE);
        if symbol.is_external() {
            for sym in symbol.all_symbols() {
                if sym.borrow().has_ast_indexes() {
                    sym.borrow_mut().ast_indexes_mut().clear(); //TODO isn't it make it invalid? should set to None?
                }
            }
            if self.file_mode {
                FileMgr::delete_path(session, &path);
            }
        } else {
            drop(symbol);
            if self.file_mode {
                session.sync_odoo.add_to_init_odoo(self.sym_stack.first().unwrap().clone());
            }
        }
    }

    fn visit_stmt(&mut self, session: &mut SessionInfo, stmt: &Stmt) {
        match stmt {
            Stmt::Import(import_stmt) => {
                self.eval_symbols_from_import_stmt(session, None, &import_stmt.names, None, &import_stmt.range)
            },
            Stmt::ImportFrom(import_from_stmt) => {
                self.eval_symbols_from_import_stmt(session, import_from_stmt.module.as_ref(), &import_from_stmt.names, Some(import_from_stmt.level), &import_from_stmt.range)
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
                self._visit_with(session, with_stmt);
            },
            Stmt::Return(return_stmt) => {
                self._visit_return(session, return_stmt);
            },
            Stmt::Match(match_stmt) => {
                self._visit_match(session, match_stmt);
            },
            Stmt::While(while_stmt) => {
                self.visit_while(session, while_stmt);
            }
            _ => {}
        }
    }

    fn eval_symbols_from_import_stmt(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) {
        if name_aliases.len() == 1 && name_aliases[0].name.to_string() == "*" {
            return;
        }
        for alias in name_aliases {
            let var_name = alias.asname.as_ref().unwrap_or(&alias.name).to_string().clone();
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&var_name, &alias.range);
            let Some(variable) = variable.clone() else {
                continue;
            };
            variable.borrow_mut().as_variable_mut().import_information = Some(ImportInformation {
                from: from_stmt.cloned(),
                level: level,
                alias: alias.clone(),
                import_step: self.current_step.clone()
            });
            //In workspace we evaluate imports as we want to raise diagnostics if not found. If outside of the workspace, let's keep only information to lazy load them
            if self.file.borrow().in_workspace() || !self.file.borrow().is_external() {
                VariableSymbol::load_from_import_information(session, variable, &self.file, &self.entry_point, &mut self.diagnostics);
            }
        }
    }

    fn _visit_ann_assign(&mut self, session: &mut SessionInfo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        for assign in assigns.iter() { //should only be one
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&assign.target.id.to_string(), &assign.target.range);
            if let Some(variable_rc) = variable {
                let parent = variable_rc.borrow().parent().unwrap().upgrade().unwrap().clone();
                if assign.annotation.is_some() {
                    let (eval, diags) = Evaluation::eval_from_ast(session, &assign.annotation.as_ref().unwrap(), parent.clone(), &ann_assign_stmt.range.start());
                    variable_rc.borrow_mut().set_evaluations(eval);
                    self.diagnostics.extend(diags);
                } else if assign.value.is_some() {
                    let (eval, diags) = Evaluation::eval_from_ast(session, &assign.value.as_ref().unwrap(), parent.clone(), &ann_assign_stmt.range.start());
                    variable_rc.borrow_mut().set_evaluations(eval);
                    self.diagnostics.extend(diags);
                } else {
                    panic!("either value or annotation should exists");
                }
                let mut dep_to_add = vec![];
                let mut v_mut = variable_rc.borrow_mut();
                let evaluations = v_mut.evaluations_mut().unwrap();
                let mut ix = 0;
                while ix < evaluations.len(){
                    let evaluation =  &evaluations[ix];
                    if let Some(sym) = evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, None).weak.upgrade() {
                        if Rc::ptr_eq(&sym, &variable_rc){
                            // TODO: investigate deps, and fix cyclic evals
                            warn!("Found cyclic evaluation symbol: {}, parent: {}", sym.borrow().name(), parent.borrow().name());
                            evaluations.remove(ix);
                            continue;
                        }
                        if let Some(file) = sym.borrow().get_file().clone() {
                            let sym_file = file.upgrade().unwrap().clone();
                            if !Rc::ptr_eq(&self.file, &sym_file) {
                                match Rc::ptr_eq(&variable_rc, &sym_file) {
                                    true => {
                                        dep_to_add.push(variable_rc.clone());
                                    },
                                    false => {
                                        dep_to_add.push(sym_file);
                                    }
                                };
                            }
                        }
                    }
                    ix += 1
                }
                for dep in dep_to_add {
                    self.file.borrow_mut().add_dependency(&mut dep.borrow_mut(), self.current_step, BuildSteps::ARCH);
                }
            } else {
                debug!("Symbol not found");
            }
        }
    }

    fn _visit_assign(&mut self, session: &mut SessionInfo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        for assign in assigns.iter() {
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&assign.target.id.to_string(), &assign.target.range);
            if let Some(variable_rc) = variable {
                let parent = variable_rc.borrow().parent().as_ref().unwrap().upgrade().unwrap().clone();
                let (eval, diags) = Evaluation::eval_from_ast(session, &assign.value.as_ref().unwrap(), parent.clone(), &assign_stmt.range.start());
                variable_rc.borrow_mut().set_evaluations(eval);
                self.diagnostics.extend(diags);
                let mut dep_to_add = vec![];
                let mut v_mut = variable_rc.borrow_mut();
                let evaluations = v_mut.evaluations_mut().unwrap();
                let mut ix = 0;
                while ix < evaluations.len(){
                    let evaluation =  &evaluations[ix];
                    if let Some(sym) = evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, None).weak.upgrade() {
                        if Rc::ptr_eq(&sym, &variable_rc){
                            // TODO: investigate deps, and fix cyclic evals
                            warn!("Found cyclic evaluation symbol: {}, parent: {}", sym.borrow().name(), parent.borrow().name());
                            evaluations.remove(ix);
                            continue;
                        }
                        if let Some(file) = sym.borrow().get_file().clone() {
                            let sym_file = file.upgrade().unwrap().clone();
                            if !Rc::ptr_eq(&self.file, &sym_file) {
                                match Rc::ptr_eq(&variable_rc, &sym_file) {
                                    true => {
                                        dep_to_add.push(variable_rc.clone());
                                    },
                                    false => {
                                        dep_to_add.push(sym_file);
                                    }
                                };
                            }
                        }
                    }
                    ix += 1
                }
                for dep in dep_to_add {
                    self.file.borrow_mut().add_dependency(&mut dep.borrow_mut(), self.current_step, BuildSteps::ARCH);
                }

            } else {
                debug!("Symbol not found");
            }
        }
    }

    fn create_diagnostic_base_not_found(&mut self, session: &mut SessionInfo, file: &mut Symbol, tree_not_found: &Tree, range: &TextRange) {
        let tree = flatten_tree(tree_not_found);
        file.not_found_paths_mut().push((BuildSteps::ARCH_EVAL, tree.clone()));
        self.entry_point.borrow_mut().not_found_symbols.insert(file.get_rc().unwrap());
        self.diagnostics.push(Diagnostic::new(
            Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
            Some(DiagnosticSeverity::WARNING),
            Some(NumberOrString::String(S!("OLS20002"))),
            Some(EXTENSION_NAME.to_string()),
            format!("{} not found", tree.join(".")),
            None,
            None,
        ));
    }

    fn load_base_classes(&mut self, session: &mut SessionInfo, loc_sym: &Rc<RefCell<Symbol>>, class_stmt: &StmtClassDef) {
        for base in class_stmt.bases() {
            let eval_base = Evaluation::eval_from_ast(session, base, self.sym_stack.last().unwrap().clone(), &class_stmt.range().start());
            self.diagnostics.extend(eval_base.1);
            let eval_base = eval_base.0;
            if eval_base.len() == 0 {
                //TODO build tree for not_found_path
                //let file = self.sym_stack[0].clone();
                //let mut file = file.borrow_mut();
                //self.create_diagnostic_base_not_found(session, &mut file, , &base.range());
                continue;
            }
            if eval_base.len() > 1 {
                self.diagnostics.push(Diagnostic::new(
                    Range::new(Position::new(base.range().start().to_u32(), 0), Position::new(base.range().end().to_u32(), 0)),
                    Some(DiagnosticSeverity::WARNING),
                    Some(NumberOrString::String(S!("OLS20005"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Multiple definition found for base class {}", AstUtils::flatten_expr(base)),
                    None,
                    None,
                ));
                continue;
            }
            let eval_base = &eval_base[0];
            let eval_symbol = eval_base.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let ref_sym = Symbol::follow_ref(&eval_symbol, session, &mut None, false, false, None, &mut vec![]);
            if ref_sym.len() > 1 {
                self.diagnostics.push(Diagnostic::new(
                    Range::new(Position::new(base.range().start().to_u32(), 0), Position::new(base.range().end().to_u32(), 0)),
                    Some(DiagnosticSeverity::WARNING),
                    Some(NumberOrString::String(S!("OLS20005"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Multiple definition found for base class {}", AstUtils::flatten_expr(base)),
                    None,
                    None,
                ));
                continue;
            }
            let symbol = &ref_sym[0].upgrade_weak();
            if let Some(symbol) = symbol {
                if symbol.borrow().typ() != SymType::COMPILED {
                    if symbol.borrow().typ() != SymType::CLASS {
                        if symbol.borrow().typ() != SymType::VARIABLE { //we followed_ref already, so if it's still a variable, it means we can't evaluate it. Skip diagnostic
                            self.diagnostics.push(Diagnostic::new(
                                Range::new(Position::new(base.start().to_u32(), 0), Position::new(base.end().to_u32(), 0)),
                                Some(DiagnosticSeverity::WARNING),
                                Some(NumberOrString::String(S!("OLS20003"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Base class {} is not a class", AstUtils::flatten_expr(base)),
                                None,
                                None,
                            ));
                        }
                    } else {
                        let file_symbol = symbol.borrow().get_file().unwrap().upgrade().unwrap();
                        if !Rc::ptr_eq(&self.file, &file_symbol) {
                            self.file.borrow_mut().add_dependency(&mut file_symbol.borrow_mut(), self.current_step, BuildSteps::ARCH);
                        }
                        loc_sym.borrow_mut().as_class_sym_mut().bases.push(Rc::downgrade(&symbol));
                    }
                }
            }
        }
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, class_stmt: &StmtClassDef) {
        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&class_stmt.name.to_string(), &class_stmt.range);
        if variable.is_none() {
            panic!("Class not found");
        }
        variable.as_ref().unwrap().borrow_mut().ast_indexes_mut().clear();
        variable.as_ref().unwrap().borrow_mut().ast_indexes_mut().extend(self.ast_indexes.iter());
        self.load_base_classes(session, variable.as_ref().unwrap(), class_stmt);
        self.sym_stack.push(variable.unwrap().clone());
        for (index, stmt) in class_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.sym_stack.pop();
    }

    fn visit_func_def(&mut self, session: &mut SessionInfo, func_stmt: &StmtFunctionDef) {
        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&func_stmt.name.to_string(), &func_stmt.range);
        if variable.is_none() {
            panic!("Function symbol not found");
        }
        let variable = variable.unwrap();
        variable.borrow_mut().ast_indexes_mut().clear();
        variable.borrow_mut().ast_indexes_mut().extend(self.ast_indexes.iter());
        {
            if variable.borrow_mut().as_func_mut().can_be_in_class() || !(self.sym_stack.last().unwrap().borrow().typ() == SymType::CLASS){
                let mut is_first = true;
                for arg in func_stmt.parameters.posonlyargs.iter().chain(&func_stmt.parameters.args) {
                    if is_first && self.sym_stack.last().unwrap().borrow().typ() == SymType::CLASS {
                        let mut var_bw = variable.borrow_mut();
                        let is_class_method = var_bw.as_func().is_class_method;
                        let symbol = var_bw.as_func_mut().symbols.get(&arg.parameter.name.id.to_string()).unwrap().get(&0).unwrap().get(0).unwrap(); //get first declaration
                        symbol.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(self.sym_stack.last().unwrap()), Some(!is_class_method)));
                        is_first = false;
                        continue;
                    }
                    is_first = false;
                    if arg.parameter.annotation.is_some() {
                        let (eval, diags) = Evaluation::eval_from_ast(session,
                                                    &arg.parameter.annotation.as_ref().unwrap(),
                                                    self.sym_stack.last().unwrap().clone(),
                                                    &func_stmt.range.start());
                        let mut var_bw = variable.borrow_mut();
                        let symbol = var_bw.as_func_mut().symbols.get(&arg.parameter.name.id.to_string()).unwrap().get(&0).unwrap().get(0).unwrap(); //get first declaration
                        symbol.borrow_mut().set_evaluations(eval);
                        self.diagnostics.extend(diags);
                    } else if arg.default.is_some() {
                        let (eval, diags) = Evaluation::eval_from_ast(session,
                                                    arg.default.as_ref().unwrap(),
                                                    self.sym_stack.last().unwrap().clone(),
                                                    &func_stmt.range.start());
                        let mut var_bw = variable.borrow_mut();
                        let symbol = var_bw.as_func_mut().symbols.get(&arg.parameter.name.id.to_string()).unwrap().get(&0).unwrap().get(0).unwrap(); //get first declaration
                        symbol.borrow_mut().set_evaluations(eval);
                        self.diagnostics.extend(diags);
                    }
                }
            } else if !variable.borrow_mut().as_func_mut().is_static{
                self.diagnostics.push(Diagnostic::new(
                    FileMgr::textRange_to_temporary_Range(&func_stmt.range),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30002"))),
                    Some(EXTENSION_NAME.to_string()),
                    S!("Non-static method should have at least one parameter"),
                    None,
                    None
                ))
            }
        }
        if !self.file_mode || variable.borrow().get_in_parents(&vec![SymType::CLASS], true).is_none() {
            variable.borrow_mut().as_func_mut().arch_eval_status = BuildStatus::IN_PROGRESS;
            PythonArchEval::handle_function_returns(session, func_stmt.returns.as_ref(), &variable, &func_stmt.range.end(), &mut self.diagnostics);
            self.sym_stack.push(variable.clone());
            for (index, stmt) in func_stmt.body.iter().enumerate() {
                self.ast_indexes.push(index as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            self.sym_stack.pop();
            PythonArchEval::handle_func_evaluations(&func_stmt.body, &variable);
            variable.borrow_mut().as_func_mut().arch_eval_status = BuildStatus::DONE;
        }
    }

    fn _visit_if(&mut self, session: &mut SessionInfo, if_stmt: &StmtIf) {
        //TODO eval test (walrus op)
        self.ast_indexes.push(0 as u16);//0 for body
        for (index, stmt) in if_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
        for (index, elif_clause) in if_stmt.elif_else_clauses.iter().enumerate() {
            //TODO eval test of else clauses
            self.ast_indexes.push((index+1) as u16);//0 for body, so index + 1
            for (index_stmt, stmt) in elif_clause.body.iter().enumerate() {
                self.ast_indexes.push(index_stmt as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            self.ast_indexes.pop();
        }
    }

    fn _visit_for(&mut self, session: &mut SessionInfo, for_stmt: &StmtFor) {
        let (eval_iter_node, diags) = Evaluation::eval_from_ast(session,
            &for_stmt.iter,
            self.sym_stack.last().unwrap().clone(),
            &for_stmt.target.range().start());
        self.diagnostics.extend(diags);
        if eval_iter_node.len() == 1 { //Only handle values that we are sure about
            let eval = &eval_iter_node[0];
            let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            if !eval_symbol.is_expired_if_weak() {
                let symbol_eval = Symbol::follow_ref(&eval_symbol, session, &mut None, false, false, None, &mut vec![]);
                if symbol_eval.len() == 1 && symbol_eval[0].upgrade_weak().is_some() {
                    let symbol_type_rc = symbol_eval[0].upgrade_weak().unwrap();
                    let symbol_type = symbol_type_rc.borrow();
                    if symbol_type.typ() == SymType::CLASS {
                        let (iter, _) = symbol_type.get_member_symbol(session, &S!("__iter__"), None, true, false, false, false);
                        if iter.len() == 1 {
                            if iter[0].borrow().evaluations().is_some() && iter[0].borrow().evaluations().unwrap().len() == 1 {
                                let iter = iter[0].borrow();
                                let eval_iter = &iter.evaluations().unwrap()[0];
                                if for_stmt.target.is_name_expr() { //only handle simple variable for now
                                    let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&for_stmt.target.as_name_expr().unwrap().id.to_string(), &for_stmt.target.range());
                                    variable.as_ref().unwrap().borrow_mut().evaluations_mut().unwrap().clear();
                                    let symbol = &eval_iter.symbol.get_symbol_as_weak(session, &mut Some(HashMap::from([(S!("parent_for"), ContextValue::SYMBOL(Rc::downgrade(&symbol_type_rc)))])), &mut vec![], None);
                                    variable.as_ref().unwrap().borrow_mut().evaluations_mut().unwrap().push(
                                        Evaluation::eval_from_symbol(
                                            &symbol.weak,
                                            symbol.instance
                                        )
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        self.ast_indexes.push(0 as u16);
        for (index_stmt, stmt) in for_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index_stmt as u16);
            self.visit_stmt(session, &stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
        //TODO split evaluation
        self.ast_indexes.push(1 as u16);
        for (index_stmt, stmt) in for_stmt.orelse.iter().enumerate() {
            self.ast_indexes.push(index_stmt as u16);
            self.visit_stmt(session, &stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
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
        self.ast_indexes.push(0 as u16);
        for (index, stmt) in try_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
        self.safe_import.pop();
        self.ast_indexes.push(1 as u16);
        for (index, stmt) in try_stmt.orelse.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
        self.ast_indexes.push(2 as u16);
        for (index, stmt) in try_stmt.finalbody.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
        self.ast_indexes.push(3 as u16);
        for (handler_iter, handler) in try_stmt.handlers.iter().enumerate() {
            self.ast_indexes.push(handler_iter as u16);
            match handler {
                ruff_python_ast::ExceptHandler::ExceptHandler(h) => {
                    for (index, stmt) in h.body.iter().enumerate() {
                        self.ast_indexes.push(index as u16);
                        self.visit_stmt(session, stmt);
                        self.ast_indexes.pop();
                    }
                },
            }
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
    }

    fn _visit_return(&mut self, session: &mut SessionInfo, return_stmt: &StmtReturn) {
        let func = self.sym_stack.last().unwrap().clone();
        if func.borrow().typ() == SymType::FUNCTION {
            if let Some(value) = return_stmt.value.as_ref() {
                let (eval, diags) = Evaluation::eval_from_ast(session, value, func.clone(), &return_stmt.range.start());
                self.diagnostics.extend(diags);
                FunctionSymbol::add_return_evaluations(func, session, eval);
            } else {
                FunctionSymbol::add_return_evaluations(func, session, vec![Evaluation::new_none()]);
            }
        }
    }

    fn _visit_with(&mut self, session: &mut SessionInfo, with_stmt: &StmtWith) {
        for item in with_stmt.items.iter() {
            if let Some(var) = item.optional_vars.as_ref() {
                match &**var {
                    Expr::Name(expr_name) => {
                        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&expr_name.id.to_string(), &expr_name.range());
                        if let Some(variable_rc) = variable {
                            let parent = variable_rc.borrow().parent().unwrap().upgrade().unwrap().clone();
                            let (eval, diags) = Evaluation::eval_from_ast(session, &item.context_expr, parent, &with_stmt.range.start());
                            let mut evals = vec![];
                            for eval in eval.iter() {
                                let symbol = eval.symbol.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, Some(variable_rc.borrow().parent_file_or_function().unwrap().upgrade().unwrap().clone()));
                                if let Some(symbol) = symbol.weak.upgrade() {
                                    let _enter_ = symbol.borrow().get_symbol(&(vec![], vec![S!("__enter__")]), u32::MAX);
                                    if let Some(_enter_) = _enter_.last() {
                                        match *_enter_.borrow() {
                                            Symbol::Function(ref func) => {
                                                evals.extend(func.evaluations.clone());
                                            },
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            variable_rc.borrow_mut().set_evaluations(eval);
                            self.diagnostics.extend(diags);
                        }
                    },
                    Expr::Tuple(_) => {continue;},
                    Expr::List(_) => {continue;},
                    _ => {continue;}
                }
            }

        }
        for (index, stmt) in with_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
    }

    fn _visit_match(&mut self, session: &mut SessionInfo<'_>, match_stmt: &ruff_python_ast::StmtMatch) {
        for (index_case, case) in match_stmt.cases.iter().enumerate() {
            self.ast_indexes.push(index_case as u16);
            for (index_body, stmt) in case.body.iter().enumerate() {
                self.ast_indexes.push(index_body as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            self.ast_indexes.pop();
        }
    }

    fn visit_while(&mut self, session: &mut SessionInfo, while_stmt: &StmtWhile) {
        self.ast_indexes.push(0 as u16); // 0 for body
        for (index, stmt) in while_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();

        self.ast_indexes.push(1 as u16); // 1 for else
        for (index, stmt) in while_stmt.orelse.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(session, stmt);
            self.ast_indexes.pop();
        }
        self.ast_indexes.pop();
    }

    // Handle function return annotation
    // Evaluate return annotation and add it to function evaluations
    fn handle_function_returns(
        session: &mut SessionInfo,
        func_returns: Option<&Box<Expr>>,
        func_sym: &Rc<RefCell<Symbol>>,
        max_infer: &TextSize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(returns_ann) = func_returns {
            let (evaluations, diags) = Evaluation::eval_from_ast(
                session,
                &returns_ann,
                func_sym.borrow().parent().and_then(|p| p.upgrade()).unwrap(),
                max_infer,
            );
            diagnostics.extend(diags);
            let file_sym = func_sym.borrow().get_file().and_then(|file_weak| file_weak.upgrade());
            // Check for type annotation `typing.Self`, if so, return a `self` evaluation
            let final_evaluations = evaluations.into_iter().map(|eval|{
                let sym_ptrs = Symbol::follow_ref(&eval.symbol.get_symbol(session, &mut None, diagnostics, None), session, &mut None, false, false, file_sym.clone(), diagnostics);
                for sym_ptr in sym_ptrs.iter(){
                    let EvaluationSymbolPtr::WEAK(sym_weak) = sym_ptr else {continue};
                    let Some(sym_rc) = sym_weak.weak.upgrade() else {continue};
                    if sym_rc.borrow().match_tree_from_any_entry(session, &(vec![S!("typing")], vec![S!("Self")])){
                        return Evaluation::new_self();
                    }
                }
                eval
            }).collect::<Vec<_>>();
            func_sym.borrow_mut().set_evaluations(final_evaluations);
        }
    }

    // Handle function evaluation if traversing the body did not get any evaluations
    // First we check if it is a function signature with no body ( like in stubs ) like def func():...
    // If so we give it an Any evaluation because it is undetermined, otherwise we give it None, becauset that means
    // we have a body but no return statement, which defaults to return None at the end
    fn handle_func_evaluations(
        func_body: &Vec<Stmt>,
        func_sym: &Rc<RefCell<Symbol>>,
    ){
        if func_sym.borrow().as_func().evaluations.is_empty() {
            let has_implementation = !matches!(
                func_body.first(),
                Some(Stmt::Expr(StmtExpr { range: _, value:  x})) if matches!(**x, Expr::EllipsisLiteral(_))
            );
            func_sym.borrow_mut().as_func_mut().evaluations  = vec![
                if has_implementation {
                    Evaluation::new_none()
                } else {
                    Evaluation::new_any()
                }
            ];
        }
    }

}
