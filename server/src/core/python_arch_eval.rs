use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::{u32, vec};

use byteyarn::{yarn, Yarn};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ruff_python_ast::{Alias, Arguments, Expr, ExprNamed, FStringPart, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtExpr, StmtFor, StmtFunctionDef, StmtIf, StmtReturn, StmtTry, StmtWhile, StmtWith};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use tracing::{debug, trace, warn};

use crate::{constants::*, Sy};
use crate::core::import_resolver::resolve_import_stmt;
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
use super::import_resolver::ImportResult;
use super::python_arch_eval_hooks::PythonArchEvalHooks;
use super::python_utils::Assign;
use super::symbols::function_symbol::FunctionSymbol;


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
            let (ast, maybe_func_stmt) = match self.file_mode {
                true => {
                    if file_info.text_hash != symbol.borrow().get_processed_text_hash(){
                        symbol.borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::INVALID);
                        return;
                    }
                    (file_info.ast.as_ref().unwrap(), None)
                },
                false => {
                    let func_stmt = AstUtils::find_stmt_from_ast(file_info.ast.as_ref().unwrap(), self.sym_stack[0].borrow().ast_indexes().unwrap()).as_function_def_stmt().unwrap();
                    (&func_stmt.body, Some(func_stmt))
                }
            };
            for (index, stmt) in ast.iter().enumerate() {
                self.ast_indexes.push(index as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            if !self.file_mode {
                self.handle_func_decorators(session, maybe_func_stmt, self.sym_stack[0].clone());
                PythonArchEval::handle_function_returns(session, maybe_func_stmt, &self.sym_stack[0], &ast.last().unwrap().range().end(), &mut self.diagnostics);
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
                        FStringPart::FString(fstr) => fstr.elements.expressions().for_each(
                            |fstring_expr| self.visit_expr(session, &fstring_expr.expression)
                        ),
                        FStringPart::Literal(_) => {},
                    }
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

    fn _match_diag_config(&self, odoo: &mut SyncOdoo, symbol: &Rc<RefCell<Symbol>>) -> bool {
        let import_diag_level = &odoo.config.diag_missing_imports;
        if *import_diag_level == DiagMissingImportsMode::None {
            return false
        }
        if *import_diag_level == DiagMissingImportsMode::All {
            return true
        }
        if *import_diag_level == DiagMissingImportsMode::OnlyOdoo {
            let tree = symbol.borrow().get_tree();
            if tree.0.len() > 0 && tree.0[0] == "odoo" {
                return true;
            }
        }
        false
    }

    ///Follow the evaluations of sym_ref, evaluate files if needed, and return true if the end evaluation contains from_sym
    fn check_for_loop_evaluation(&mut self, session: &mut SessionInfo, sym_ref: Rc<RefCell<Symbol>>, from_sym: &Rc<RefCell<Symbol>>) -> bool {
        let sym_ref_cl = sym_ref.clone();
        let syms_followed = Symbol::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
            Rc::downgrade(&sym_ref_cl), None, false
        )), session, &mut None, false, false, None, &mut self.diagnostics);
        for sym in syms_followed.iter() {
            let sym = sym.upgrade_weak();
            if let Some(sym) = sym {
                if sym.borrow().evaluations().is_some() && sym.borrow().evaluations().unwrap().is_empty() {
                    let file_sym = sym_ref.borrow().get_file();
                    if file_sym.is_some() {
                        let rc_file_sym = file_sym.as_ref().unwrap().upgrade().unwrap();
                        if rc_file_sym.borrow_mut().build_status(BuildSteps::ARCH_EVAL) == BuildStatus::PENDING && session.sync_odoo.is_in_rebuild(&rc_file_sym, BuildSteps::ARCH_EVAL) {
                            session.sync_odoo.remove_from_rebuild_arch_eval(&rc_file_sym);
                            let mut builder = PythonArchEval::new(self.entry_point.clone(), rc_file_sym);
                            builder.eval_arch(session);
                            if self.check_for_loop_evaluation(session, sym_ref.clone(), from_sym) {
                                return true;
                            }
                        }
                    }
                }
                if Rc::ptr_eq(&sym, &from_sym) {
                    return true;
                }
            }
        }
        false
    }

    fn eval_symbols_from_import_stmt(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) {
        if name_aliases.len() == 1 && name_aliases[0].name.to_string() == "*" {
            return;
        }
        let import_results: Vec<ImportResult> = resolve_import_stmt(
            session,
            &self.file,
            from_stmt,
            name_aliases,
            level,
            &mut Some(&mut self.diagnostics));

        for _import_result in import_results.iter() {
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&_import_result.name, &_import_result.range);
            let Some(variable) = variable.clone() else {
                continue;
            };
            if _import_result.found {
                let import_sym_ref = _import_result.symbol.clone();
                let has_loop = self.check_for_loop_evaluation(session, import_sym_ref, &variable);
                if !has_loop { //anti-loop. We want to be sure we are not evaluating to the same sym
                    variable.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(&_import_result.symbol), None)]);
                    let file_of_import_symbol = _import_result.symbol.borrow().get_file();
                    if let Some(import_file) = file_of_import_symbol {
                        let import_file = import_file.upgrade().unwrap();
                        if !Rc::ptr_eq(&self.file, &import_file) {
                            self.file.borrow_mut().add_dependency(&mut import_file.borrow_mut(), self.current_step, BuildSteps::ARCH);
                        }
                    }
                } else {
                    let mut file_tree = [_import_result.file_tree.0.clone(), _import_result.file_tree.1.clone()].concat();
                    file_tree.extend(_import_result.name.split(".").map(|s| yarn!("{}", s)));
                    self.file.borrow_mut().not_found_paths_mut().push((self.current_step, file_tree.clone()));
                    self.entry_point.borrow_mut().not_found_symbols.insert(self.file.clone());
                    if self._match_diag_config(session.sync_odoo, &_import_result.symbol) {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(_import_result.range.start().to_u32(), 0), Position::new(_import_result.range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::WARNING),
                            Some(NumberOrString::String(S!("OLS20004"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Failed to evaluate import {}", file_tree.clone().join(".")),
                            None,
                            None,
                        ));
                    }
                }

            } else {
                let mut file_tree = [_import_result.file_tree.0.clone(), _import_result.file_tree.1.clone()].concat();
                file_tree.extend(_import_result.name.split(".").map(|s| yarn!("{}", s)));
                if BUILT_IN_LIBS.contains(&file_tree[0].as_str()) {
                    continue;
                }
                if !self.safe_import.last().unwrap() {
                    self.file.borrow_mut().not_found_paths_mut().push((self.current_step, file_tree.clone()));
                    self.entry_point.borrow_mut().not_found_symbols.insert(self.file.clone());
                    if self._match_diag_config(session.sync_odoo, &_import_result.symbol) {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(_import_result.range.start().to_u32(), 0), Position::new(_import_result.range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::WARNING),
                            Some(NumberOrString::String(S!("OLS20001"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("{} not found", file_tree.clone().join(".")),
                            None,
                            None,
                        ));
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
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&Yarn::from(assign.target.id.to_string()), &assign.target.range);
            if let Some(variable_rc) = variable {
                let parent = variable_rc.borrow().parent().unwrap().upgrade().unwrap().clone();
                let (eval, diags) = if let Some(ref annotation) = assign.annotation {
                    Evaluation::eval_from_ast(session, annotation, parent.clone(), &range.start())
                } else if let Some(ref value) = assign.value {
                    Evaluation::eval_from_ast(session, value, parent.clone(), &range.start())
                } else {
                    panic!("either value or annotation should exists");
                };
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

    fn  _visit_ann_assign(&mut self, session: &mut SessionInfo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        self.handle_assigns(session, assigns, &ann_assign_stmt.range);
    }

    fn _visit_assign(&mut self, session: &mut SessionInfo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        self.handle_assigns(session, assigns, &assign_stmt.range);
    }

    fn visit_named_expr(&mut self, session: &mut SessionInfo, named_expr: &ExprNamed) {
        let assigns = python_utils::unpack_assign(&vec![*named_expr.target.clone()], None, Some(&named_expr.value));
        self.handle_assigns(session, assigns, &named_expr.range);
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
        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&Yarn::from(class_stmt.name.to_string()), &class_stmt.range);
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
        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&Yarn::from(func_stmt.name.to_string()), &func_stmt.range);
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
                        let symbol = var_bw.as_func_mut().symbols.get(&Yarn::from(arg.parameter.name.id.to_string())).unwrap().get(&0).unwrap().get(0).unwrap(); //get first declaration
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
                        let symbol = var_bw.as_func_mut().symbols.get(&Yarn::from(arg.parameter.name.id.to_string())).unwrap().get(&0).unwrap().get(0).unwrap(); //get first declaration
                        symbol.borrow_mut().set_evaluations(eval);
                        self.diagnostics.extend(diags);
                    } else if arg.default.is_some() {
                        let (eval, diags) = Evaluation::eval_from_ast(session,
                                                    arg.default.as_ref().unwrap(),
                                                    self.sym_stack.last().unwrap().clone(),
                                                    &func_stmt.range.start());
                        let mut var_bw = variable.borrow_mut();
                        let symbol = var_bw.as_func_mut().symbols.get(&Yarn::from(arg.parameter.name.id.to_string())).unwrap().get(&0).unwrap().get(0).unwrap(); //get first declaration
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
            self.sym_stack.push(variable.clone());
            for (index, stmt) in func_stmt.body.iter().enumerate() {
                self.ast_indexes.push(index as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            self.sym_stack.pop();
            PythonArchEval::handle_function_returns(session, Some(func_stmt), &variable, &func_stmt.range.end(), &mut self.diagnostics);
            PythonArchEval::handle_func_evaluations(&func_stmt.body, &variable);
            variable.borrow_mut().as_func_mut().arch_eval_status = BuildStatus::DONE;
        }
    }

    fn _visit_if(&mut self, session: &mut SessionInfo, if_stmt: &StmtIf) {
        self.visit_expr(session, &if_stmt.test);
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
                elif_clause.test.as_ref().map(|test_clause| self.visit_expr(session, &test_clause));
                self.ast_indexes.push(index_stmt as u16);
                self.visit_stmt(session, stmt);
                self.ast_indexes.pop();
            }
            self.ast_indexes.pop();
        }
    }

    fn _visit_for(&mut self, session: &mut SessionInfo, for_stmt: &StmtFor) {
        self.visit_expr(session, &for_stmt.iter);
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
                                    let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&Yarn::from(for_stmt.target.as_name_expr().unwrap().id.to_string()), &for_stmt.target.range());
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
                    h.type_.as_ref().map(|test_clause| self.visit_expr(session, test_clause));
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
        if let Some(value) = return_stmt.value.as_ref() {
            self.visit_expr(session, &value);
        }
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
            self.visit_expr(session, &item.context_expr);
            if let Some(var) = item.optional_vars.as_ref() {
                match &**var {
                    Expr::Name(expr_name) => {
                        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&Yarn::from(expr_name.id.to_string()), &expr_name.range());
                        if let Some(variable_rc) = variable {
                            let parent = variable_rc.borrow().parent().unwrap().upgrade().unwrap().clone();
                            let (eval, diags) = Evaluation::eval_from_ast(session, &item.context_expr, parent, &with_stmt.range.start());
                            let mut evals = vec![];
                            for eval in eval.iter() {
                                let symbol = eval.symbol.get_symbol_as_weak(session, &mut None, &mut self.diagnostics, Some(variable_rc.borrow().parent_file_or_function().unwrap().upgrade().unwrap().clone()));
                                if let Some(symbol) = symbol.weak.upgrade() {
                                    let _enter_ = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__enter__")]), u32::MAX);
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
            case.guard.as_ref().map(|test_clause| self.visit_expr(session, test_clause));
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
        self.visit_expr(session, &while_stmt.test);
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
        func_stmt: Option<&StmtFunctionDef>,
        func_sym: &Rc<RefCell<Symbol>>,
        max_infer: &TextSize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(returns_ann) = func_stmt.and_then(|func_stmt| func_stmt.returns.as_ref()) {
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
                    if sym_rc.borrow().match_tree_from_any_entry(session, &(vec![Sy!("typing")], vec![Sy!("Self")])){
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

    fn handle_api_returns_decorator(&mut self, session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments){
        let Some(Expr::StringLiteral(expr)) = arguments.args.first() else {return};
        let returns_str = expr.value.to_string();
        if returns_str == S!("self"){
            func_sym.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
            return;
        }
        let Some(model) = session.sync_odoo.models.get(&yarn!("{}", returns_str)).cloned() else {
            self.diagnostics.push(Diagnostic::new(
                FileMgr::textRange_to_temporary_Range(&expr.range()),
                Some(DiagnosticSeverity::ERROR),
                Some(NumberOrString::String(S!("OLS30102"))),
                Some(EXTENSION_NAME.to_string()),
                S!("Unknown model. Check your addons path"),
                None,
                None,
            ));
            return;
        };
        let Some(ref main_model_sym) =  model.borrow().get_main_symbols(session, func_sym.borrow().find_module()).first().cloned() else {
            self.diagnostics.push(Diagnostic::new(
                FileMgr::textRange_to_temporary_Range(&expr.range()),
                Some(DiagnosticSeverity::ERROR),
                Some(NumberOrString::String(S!("OLS30101"))),
                Some(EXTENSION_NAME.to_string()),
                S!("This model is not in the dependencies of your module."),
                None,
                None,
            ));
            return
        };
        func_sym.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(main_model_sym), Some(false))]);
    }

    /// For @api.constrains and @api.onchange, both can only take a simple field name
    fn handle_api_simple_field_decorator(&mut self, session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments){
        let from_module = func_sym.borrow().find_module();

        let Some(class_sym) = func_sym.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(
            |class_sym_weak| class_sym_weak.upgrade()
        ) else {
            return;
        };

        let Some(model_name) = class_sym.borrow().as_class_sym()._model.as_ref().map(|model| &model.name).cloned() else {
            return;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return};
            let field_name = expr.value.to_string();
            let (syms, _) = class_sym.borrow().get_member_symbol(session, &field_name, from_module.clone(), false, false, true, false);
            if syms.is_empty(){
                self.diagnostics.push(Diagnostic::new(
                    FileMgr::textRange_to_temporary_Range(&expr.range()),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30323"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Field {field_name} does not exist on model {model_name}"),
                    None,
                    None,
                ));
            }
        }
    }

    pub fn get_nested_sub_field(
        session: &mut SessionInfo,
        field_name: &String,
        class_sym: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
    ) -> Vec<Rc<RefCell<Symbol>>>{
        let mut parent_object = Some(class_sym);
        let mut syms = vec![];
        let split_expr: Vec<String> = field_name.split(".").map(|x| x.to_string()).collect();
        for (ix, name) in split_expr.iter().enumerate() {
            if parent_object.is_none() {
                break;
            }
            let (symbols, _diagnostics) = parent_object.clone().unwrap().borrow().get_member_symbol(session,
                &name.to_string(),
                from_module.clone(),
                false,
                true,
                true,
                false);
            if ix == split_expr.len() - 1 {
                syms = symbols;
                break;
            } else if symbols.is_empty() {
                break;
            }
            parent_object = None;
            for s in symbols.iter() {
                if !s.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) {
                    break;
                }
                let models = s.borrow().as_variable().get_relational_model(session, from_module.clone());
                if models.len() == 1 {
                    parent_object = Some(models[0].clone());
                    break;
                }
            }
        }
        syms
    }

    /// For @api.depends, which can take a nested simple field name
    fn handle_api_nested_field_decorator(&mut self, session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments){
        let from_module = func_sym.borrow().find_module();

        let Some(class_sym) = func_sym.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(
            |class_sym_weak| class_sym_weak.upgrade()
        ) else {
            return;
        };

        let Some(model_name) = class_sym.borrow().as_class_sym()._model.as_ref().map(|model| &model.name).cloned() else {
            return;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return};
            let field_name = expr.value.to_string();
            let syms = PythonArchEval::get_nested_sub_field(session, &field_name, class_sym.clone(), from_module.clone());
            if syms.is_empty(){
                self.diagnostics.push(Diagnostic::new(
                    FileMgr::textRange_to_temporary_Range(&expr.range()),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30323"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Field {field_name} does not exist on model {model_name}"),
                    None,
                    None,
                ));
            }
        }
    }

    /// Read function decorators and set evaluations where applicable
    /// - api.returns -> self -> Self, string -> model name if exists
    fn handle_func_decorators(
        &mut self,
        session: &mut SessionInfo,
        maybe_func_stmt: Option<&StmtFunctionDef>,
        func_sym: Rc<RefCell<Symbol>>,
    ){
        let Some(func_stmt) = maybe_func_stmt else {return};
        for decorator in func_stmt.decorator_list.iter(){
            let (decorator_base, decorator_args) = match &decorator.expression {
                Expr::Call(call_expr) => {
                    (&call_expr.func, &call_expr.arguments)
                },
                _ => {continue;}
            };
            if decorator_args.args.is_empty(){
                continue; // All the decorators we handle have at least one arg for now
            }
            let (dec_evals, diags) = Evaluation::eval_from_ast(session, &decorator_base, self.sym_stack.last().unwrap().clone(), &func_stmt.range.start());
            self.diagnostics.extend(diags);
            for decorator_eval in dec_evals.iter(){
                let EvaluationSymbolPtr::WEAK(decorator_eval_sym_weak) = decorator_eval.symbol.get_symbol(session, &mut None, &mut self.diagnostics, None)  else {continue};
                let Some(dec_sym) = decorator_eval_sym_weak.weak.upgrade() else {continue};
                let dec_sym_tree = dec_sym.borrow().get_main_entry_tree(session);
                if dec_sym_tree == (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("returns")]){
                    self.handle_api_returns_decorator(session, func_sym.clone(), decorator_args);
                } else if dec_sym_tree == (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("onchange")]) ||
                        dec_sym_tree == (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("constrains")]){
                    self.handle_api_simple_field_decorator(session, func_sym.clone(), decorator_args);
                } else if dec_sym_tree == (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("depends")]){
                    self.handle_api_nested_field_decorator(session, func_sym.clone(), decorator_args);
                }
            }
        }
    }

}
