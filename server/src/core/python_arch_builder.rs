use std::rc::{Rc, Weak};
use std::cell::RefCell;
use anyhow::{Error};
use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Alias, Identifier, Int, Stmt, StmtAnnAssign, StmtAssign, Constant};
use std::path::PathBuf;

use crate::constants::SymType;
use crate::core::python_utils;
use crate::FILE_MGR;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::core::evaluation::{Evaluation, EvaluationValue};

use super::import_resolver::ImportResult;


#[derive(Debug)]
pub struct PythonArchBuilder {
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    __all_symbols_to_add: Vec<Symbol>,
}

impl PythonArchBuilder {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonArchBuilder {
        PythonArchBuilder {
            sym_stack: vec![symbol],
            __all_symbols_to_add: Vec::new(),
        }
    }

    pub fn load_arch(&mut self, odoo: &mut SyncOdoo) -> Result<(), Error> {
        println!("load arch");
        let mut temp = FILE_MGR.lock().unwrap();
        let symbol = self.sym_stack[0].borrow_mut();
        if symbol.paths.len() != 1 {
            panic!()
        }
        let mut path = symbol.paths[0].clone();
        println!("path: {}", path);
        if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + symbol.i_ext.as_str();
        }
        drop(symbol);
        let mut file_info = temp.get_file_info(path.as_str()); //create ast
        match file_info.ast {
            Some(_) => {},
            None => {
                file_info.build_ast(path.as_str(), "");
            }
        }
        if file_info.ast.is_some() {
            for stmt in file_info.ast.as_ref().unwrap() {
                match stmt {
                    Stmt::Import(import_stmt) => {
                        self.create_local_symbols_from_import_stmt(odoo, None, &import_stmt.names, None, &import_stmt.range)?
                    },
                    Stmt::ImportFrom(import_from_stmt) => {
                        self.create_local_symbols_from_import_stmt(odoo, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level.as_ref(), &import_from_stmt.range)?
                    },
                    Stmt::AnnAssign(ann_assign_stmt) => {
                        self._visit_ann_assign(odoo, ann_assign_stmt);
                    },
                    Stmt::Assign(assign_stmt) => {
                        self._visit_assign(odoo, assign_stmt);
                    },
                    Stmt::FunctionDef(function_def_stmt) => {

                    },
                    Stmt::ClassDef(class_def_stmt) => {

                    },
                    _ => {}
                }
            }
            self._resolve_all_symbols(odoo);
            odoo.add_to_rebuild_arch_eval(Rc::downgrade(&self.sym_stack[0]));
        }
        Ok(())
    }

    fn create_local_symbols_from_import_stmt(&self, odoo: &mut SyncOdoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, range: &TextRange) -> Result<(), Error> {
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
                    //TODO add to not found symbols
                    continue;
                }
                let allowed_names = true;
                if import_result.symbol.borrow_mut().symbols.contains_key("__all__") {
                    // TODO implement __all__ imports
                }
                for s in import_result.symbol.borrow_mut().symbols.values() {
                    let mut variable = Symbol::new(s.borrow_mut().name.clone(), SymType::VARIABLE); //TODO mark as import
                    variable.range = Some(import_name.range.clone());
                    variable.evaluation = Some(Evaluation::eval_from_symbol(&s));
                    //TODO add dependency
                    self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, variable);
                }

            } else {
                let var_name = if import_name.asname.is_none() {
                    import_name.name.clone()
                } else {
                    import_name.asname.as_ref().unwrap().clone()
                };
                let mut variable = Symbol::new(var_name.to_string(), SymType::VARIABLE); //TODO mark as import
                variable.range = Some(import_name.range.clone());
                self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, variable);
            }
        }
        Ok(())
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
                    variable.evaluation = Evaluation::eval_from_ast(odoo, &assign.value.as_ref().unwrap(), parent);
                    if variable.evaluation.is_some() {
                        //TODO add dependency
                        if (*self.sym_stack.last().unwrap()).borrow().is_external {
                            // external packages often import symbols from compiled files
                            // or with meta programmation like globals["var"] = __get_func().
                            // we don't want to handle that, so just declare __all__ content
                            // as symbols to not raise any error.
                            let evaluation = variable.evaluation.as_ref().unwrap();
                            let evaluated = evaluation.get_symbol().upgrade();
                            if evaluated.is_some() {
                                let evaluated = evaluated.unwrap();
                                let evaluated = evaluated.borrow();
                                if evaluated.sym_type == SymType::CONSTANT {
                                    match evaluation.value.as_ref().unwrap() {
                                        EvaluationValue::LIST(list) => {
                                            for item in list.iter() {
                                                match item {
                                                    Constant::Str(s) => {
                                                        let mut var = Symbol::new(s.to_string(), SymType::VARIABLE);
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
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn _resolve_all_symbols(&mut self, odoo: &mut SyncOdoo) {
        for symbol in self.__all_symbols_to_add.drain(..) {
            self.sym_stack.last().unwrap().borrow_mut().add_symbol(odoo, symbol);
        }
    }
}