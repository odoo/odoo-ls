use std::collections::HashSet;
use std::rc::Rc;
use std::cell::RefCell;
use std::vec;
use anyhow::Error;
use ruff_text_size::{Ranged, TextRange};
use ruff_python_ast::{Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtIf, StmtTry};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use weak_table::traits::WeakElement;
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;

use crate::constants::{SymType, BuildStatus, BuildSteps};
use crate::core::python_utils;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::core::evaluation::{Evaluation, EvaluationValue};
use crate::core::python_arch_builder_hooks::PythonArchBuilderHooks;
use crate::S;

use super::import_resolver::ImportResult;
use super::symbols::class_symbol::ClassSymbol;
use super::symbols::function_symbol::FunctionSymbol;


#[derive(Debug)]
pub struct PythonArchBuilder {
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    __all_symbols_to_add: Vec<Symbol>,
    diagnostics: Vec<Diagnostic>
}

impl PythonArchBuilder {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonArchBuilder {
        PythonArchBuilder {
            sym_stack: vec![symbol],
            __all_symbols_to_add: Vec::new(),
            diagnostics: vec![]
        }
    }

    pub fn load_arch(&mut self, odoo: &mut SyncOdoo) -> Result<(), Error> {
        //println!("load arch");
        let mut symbol = self.sym_stack[0].borrow_mut();
        symbol.arch_status = BuildStatus::IN_PROGRESS;
        if symbol.paths.len() != 1 {
            panic!()
        }
        let mut path = symbol.paths[0].clone();
        //println!("load arch path: {}", path);
        if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + symbol.i_ext.as_str();
        }
        symbol.in_workspace = (symbol.parent.is_some() &&
            symbol.parent.as_ref().unwrap().upgrade().is_some() &&
            symbol.parent.as_ref().unwrap().upgrade().unwrap().borrow().in_workspace) ||
            odoo.get_file_mgr().borrow().is_in_workspace(path.as_str());
        drop(symbol);
        let file_info = odoo.get_file_mgr().borrow_mut().update_file_info(odoo, path.as_str(), None, None, false); //create ast if not in cache
        let mut file_info = (*file_info).borrow_mut();
        file_info.replace_diagnostics(BuildSteps::ARCH, self.diagnostics.clone());
        if file_info.ast.is_some() {
            self.visit_node(odoo, &file_info.ast.as_ref().unwrap())?;
            self._resolve_all_symbols(odoo);
            odoo.add_to_rebuild_arch_eval(self.sym_stack[0].clone());
        } else {
            file_info.publish_diagnostics(odoo);
        }
        let mut symbol = self.sym_stack[0].borrow_mut();
        symbol.arch_status = BuildStatus::DONE;
        Ok(())
    }

    fn create_local_symbols_from_import_stmt(&mut self, odoo: &mut SyncOdoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) -> Result<(), Error> {
        for import_name in name_aliases {
            if import_name.name.as_str() == "*" {
                if self.sym_stack.len() != 1 { //only at top level for now.
                    continue;
                }
                let import_result: ImportResult = resolve_import_stmt(
                    odoo,
                    self.sym_stack.last().unwrap(),
                    self.sym_stack.last().unwrap(),
                    from_stmt,
                    name_aliases,
                    level,
                    range).remove(0); //we don't need the vector with this call as there will be 1 result.
                if !import_result.found {
                    odoo.not_found_symbols.insert(self.sym_stack[0].clone());
                    let file_tree_flattened = vec![import_result.file_tree.0.clone(), import_result.file_tree.1.clone()].concat();
                    self.sym_stack[0].borrow_mut().not_found_paths.push((BuildSteps::ARCH, file_tree_flattened));
                    continue;
                }
                let mut all_name_allowed = true;
                let mut name_filter: Vec<String> = vec![];
                if import_result.symbol.borrow_mut().symbols.contains_key("__all__") {
                    let all = import_result.symbol.borrow_mut().symbols["__all__"].clone();
                    let all = Symbol::follow_ref(all, odoo, &mut None, false, false, &mut self.diagnostics).0;
                    if all.is_expired() || (*all.upgrade().unwrap()).borrow().evaluation.is_none() ||
                        !(*all.upgrade().unwrap()).borrow().evaluation.as_ref().unwrap().value.is_some() {
                            println!("invalid __all__ import in file {}", (*import_result.symbol).borrow().paths[0] )
                    } else {
                        let all = all.upgrade().unwrap();
                        let all = (*all).borrow();
                        let value = &all.evaluation.as_ref().unwrap().value;
                        let (nf, parse_error) = self.extract_all_symbol_eval_values(&value.as_ref());
                        if parse_error {
                            println!("error during parsing __all__ import in file {}", (*import_result.symbol).borrow().paths[0] )
                        }
                        name_filter = nf;
                        all_name_allowed = false;
                    }
                }
                for s in import_result.symbol.borrow_mut().symbols.values() {
                    if all_name_allowed || name_filter.contains(&s.borrow().name) {
                        let mut variable = Symbol::new(s.borrow_mut().name.clone(), SymType::VARIABLE);
                        variable.is_import_variable = true;
                        variable.range = Some(import_name.range.clone());
                        variable.evaluation = Some(Evaluation::eval_from_symbol(&s));
                        if variable.evaluation.is_some() {
                            let evaluation = variable.evaluation.as_ref().unwrap();
                            let evaluated_type = &evaluation.symbol;
                            let evaluated_type = evaluated_type.get_symbol(odoo, &mut None, &mut self.diagnostics).0.upgrade();
                            if evaluated_type.is_some() {
                                let evaluated_type = evaluated_type.unwrap();
                                self.sym_stack[0].borrow_mut().add_dependency(&mut evaluated_type.borrow_mut(), BuildSteps::ARCH, BuildSteps::ARCH);
                            }
                        }
                        self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, variable);
                    }
                }

            } else {
                let var_name = if import_name.asname.is_none() {
                    S!(import_name.name.split(".").next().unwrap().clone())
                } else {
                    import_name.asname.as_ref().unwrap().clone().to_string()
                };
                let mut variable = Symbol::new(var_name.to_string(), SymType::VARIABLE);
                variable.is_import_variable = true;
                variable.range = Some(import_name.range.clone());
                self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, variable);
            }
        }
        Ok(())
    }

    fn visit_node(&mut self, odoo: &mut SyncOdoo, nodes: &Vec<Stmt>) -> Result<(), Error> {
        for stmt in nodes.iter() {
            match stmt {
                Stmt::Import(import_stmt) => {
                    if self.sym_stack.last().unwrap().borrow().sym_type != SymType::FUNCTION {
                        self.create_local_symbols_from_import_stmt(odoo, None, &import_stmt.names, None, &import_stmt.range)?
                    }
                },
                Stmt::ImportFrom(import_from_stmt) => {
                    if self.sym_stack.last().unwrap().borrow().sym_type != SymType::FUNCTION {
                        self.create_local_symbols_from_import_stmt(odoo, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level, &import_from_stmt.range)?
                    }
                },
                Stmt::AnnAssign(ann_assign_stmt) => {
                    if self.sym_stack.last().unwrap().borrow().sym_type != SymType::FUNCTION {
                        self._visit_ann_assign(odoo, ann_assign_stmt);
                    }
                },
                Stmt::Assign(assign_stmt) => {
                    if self.sym_stack.last().unwrap().borrow().sym_type != SymType::FUNCTION {
                        self._visit_assign(odoo, assign_stmt);
                    }
                },
                Stmt::FunctionDef(function_def_stmt) => {
                    self.visit_func_def(odoo, function_def_stmt)?;
                },
                Stmt::ClassDef(class_def_stmt) => {
                    self.visit_class_def(odoo, class_def_stmt)?;
                },
                Stmt::If(if_stmt) => {
                    self.visit_if(odoo, if_stmt);
                },
                Stmt::Try(try_stmt) => {
                    self.visit_try(odoo, try_stmt);
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
                    EvaluationValue::CONSTANT(c) => {
                        match c {
                            Expr::StringLiteral(s) => {
                                vec![s.value.to_string()]
                            },
                            _ => {parse_error = true; vec![]}
                        }
                    },
                    EvaluationValue::DICT(d) => {
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

    fn _visit_ann_assign(&mut self, odoo: &mut SyncOdoo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        for assign in assigns.iter() { //should only be one
            let mut variable = Symbol::new(assign.target.id.to_string(), SymType::VARIABLE);
            variable.range = Some(assign.target.range.clone());
            variable.evaluation = None;
            self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, variable);
        }
    }

    fn _visit_assign(&mut self, odoo: &mut SyncOdoo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        for assign in assigns.iter() {
            let mut variable = Symbol::new(assign.target.id.to_string(), SymType::VARIABLE);
            variable.range = Some(assign.target.range.clone());
            variable.evaluation = None;
            let variable = self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, variable);
            let mut variable = variable.borrow_mut();
            if variable.name == "__all__" && assign.value.is_some() && variable.parent.is_some() {
                let parent = variable.parent.as_ref().unwrap().upgrade();
                if parent.is_some() {
                    let parent = parent.unwrap();
                    let eval = Evaluation::eval_from_ast(odoo, &assign.value.as_ref().unwrap(), parent, &assign_stmt.range.start());
                    variable.evaluation = eval.0;
                    self.diagnostics.extend(eval.1);
                    if variable.evaluation.is_some() {
                        if (*self.sym_stack.last().unwrap()).borrow().is_external {
                            // external packages often import symbols from compiled files
                            // or with meta programmation like globals["var"] = __get_func().
                            // we don't want to handle that, so just declare __all__ content
                            // as symbols to not raise any error.
                            let evaluation = variable.evaluation.as_ref().unwrap();
                            let evaluated = &evaluation.symbol;
                            let evaluated = evaluated.get_symbol(odoo, &mut None, &mut self.diagnostics).0.upgrade();
                            if evaluated.is_some() {
                                let evaluated = evaluated.unwrap();
                                let evaluated = evaluated.borrow();
                                if evaluated.sym_type == SymType::CONSTANT && evaluated.evaluation.is_some() && evaluated.evaluation.as_ref().unwrap().value.is_some() {
                                    match evaluated.evaluation.as_ref().unwrap().value.as_ref().unwrap() {
                                        EvaluationValue::LIST(list) => {
                                            for item in list.iter() {
                                                match item {
                                                    Expr::StringLiteral(s) => {
                                                        let mut var = Symbol::new(s.value.to_string(), SymType::VARIABLE);
                                                        var.range = evaluated.range.clone();
                                                        var.evaluation = None;
                                                        self.__all_symbols_to_add.push(var);
                                                    },
                                                    _ => {}
                                                }
                                            }
                                        },
                                        _ => {}
                                    }
                                } else {
                                    println!("__all__ symbol to analyze");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn visit_func_def(&mut self, odoo: &mut SyncOdoo, func_def: &StmtFunctionDef) -> Result<(), Error> {
        let mut sym = Symbol::new(func_def.name.to_string(), SymType::FUNCTION);
        sym.range = Some(func_def.range.clone());
        sym._function = Some(FunctionSymbol{
            is_static: false,
            is_property: false,
            diagnostics: vec![]
        });
        for decorator in func_def.decorator_list.iter() {
            if decorator.expression.is_name_expr() && decorator.expression.as_name_expr().unwrap().id.to_string() == "staticmethod" {
                sym._function.as_mut().unwrap().is_static = true;
            }
            if decorator.expression.is_name_expr() && decorator.expression.as_name_expr().unwrap().id.to_string() == "property" {
                sym._function.as_mut().unwrap().is_property = true;
            }
        }
        if func_def.body.len() > 0 && func_def.body[0].is_expr_stmt() {
            let expr: &ruff_python_ast::StmtExpr = func_def.body[0].as_expr_stmt().unwrap();
            if let Some(s) = expr.value.as_string_literal_expr() {
                sym.doc_string = Some(s.value.to_string())
            }
        }
        let sym = (*self.sym_stack.last().unwrap()).borrow_mut().add_symbol(odoo, sym);
        //add params
        for arg in func_def.parameters.args.iter() {
            let mut param = Symbol::new(arg.parameter.name.id.clone(), SymType::VARIABLE);
            param.range = Some(arg.range);
            sym.borrow_mut().add_symbol_to_locals(odoo, param);
        }
        //visit body
        self.sym_stack.push(sym);
        self.visit_node(odoo, &func_def.body)?;
        self.sym_stack.pop();
        Ok(())
    }

    fn visit_class_def(&mut self, odoo: &mut SyncOdoo, class_def: &StmtClassDef) -> Result<(), Error> {
        let mut sym = Symbol::new(class_def.name.to_string(), SymType::CLASS);
        sym._class = Some(ClassSymbol {
            bases: PtrWeakHashSet::new(),
            diagnostics: vec![]
        });
        sym.range = Some(class_def.range.clone());
        if class_def.body.len() > 0 && class_def.body[0].is_expr_stmt() {
            let expr = class_def.body[0].as_expr_stmt().unwrap();
            if expr.value.is_literal_expr() {
                let const_expr = expr.value.as_literal_expr().unwrap();
                if let Some(s) = const_expr.as_string_literal() {
                    sym.doc_string = Some(s.value.to_string());
                }
            }
        }
        sym.evaluation = None;
        let sym = (*self.sym_stack.last().unwrap()).borrow_mut().add_symbol(odoo, sym);
        self.sym_stack.push(sym.clone());
        self.visit_node(odoo, &class_def.body)?;
        self.sym_stack.pop();
        PythonArchBuilderHooks::on_class_def(odoo, sym);
        Ok(())
    }

    fn _resolve_all_symbols(&mut self, odoo: &mut SyncOdoo) {
        for symbol in self.__all_symbols_to_add.drain(..) {
            if !self.sym_stack.last().unwrap().borrow().symbols.contains_key(&symbol.name) {
                self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, symbol);
            }
        }
    }

    fn visit_if(&mut self, odoo: &mut SyncOdoo, if_stmt: &StmtIf) -> Result<(), Error> {
        //TODO check platform condition (sys.version > 3.12, etc...)
        self.visit_node(odoo, &if_stmt.body)?;
        for else_clause in if_stmt.elif_else_clauses.iter() {
            self.visit_node(odoo, &else_clause.body)?;
        }
        Ok(())
    }

    fn visit_try(&mut self, odoo: &mut SyncOdoo, try_stmt: &StmtTry) -> Result<(), Error> {
        self.visit_node(odoo, &try_stmt.body)?;
        self.visit_node(odoo, &try_stmt.orelse)?;
        self.visit_node(odoo, &try_stmt.finalbody)?;
        Ok(())
    }
}