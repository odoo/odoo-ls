use std::rc::{Rc, Weak};
use std::cell::RefCell;

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Stmt, Alias, Int, StmtAnnAssign, StmtAssign};
use std::path::PathBuf;

use crate::constants::*;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::core::evaluation::Evaluation;
use crate::core::python_utils;

use super::import_resolver::ImportResult;


#[derive(Debug, Clone)]
pub struct PythonArchEval {
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
}

impl PythonArchEval {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonArchEval {
        PythonArchEval {
            sym_stack: vec![symbol]
        }
    }

    pub fn eval_arch(&mut self, odoo: &mut SyncOdoo) {
        //println!("eval arch");
        let mut symbol = self.sym_stack[0].borrow_mut();
        symbol.arch_eval_status = BuildStatus::IN_PROGRESS;
        if symbol.paths.len() != 1 {
            panic!()
        }
        let mut path = symbol.paths[0].clone();
        //println!("path: {}", path);
        if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + symbol.i_ext.as_str();
        }
        drop(symbol);
        let file_info = odoo.file_mgr.get_file_info(path.as_str()); //create ast
        let file_info = (*file_info).borrow();
        if file_info.ast.is_some() {
            for stmt in file_info.ast.as_ref().unwrap() {
                match stmt {
                    //TODO move import logic from ast visiting to symbol analyzing
                    Stmt::Import(import_stmt) => {
                        self.eval_local_symbols_from_import_stmt(odoo, None, &import_stmt.names, None, &import_stmt.range)
                    },
                    Stmt::ImportFrom(import_from_stmt) => {
                        self.eval_local_symbols_from_import_stmt(odoo, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level.as_ref(), &import_from_stmt.range)
                    },
                    _ => {}
                }
            }
        }
        let mut symbol = self.sym_stack[0].borrow_mut();
        symbol.arch_eval_status = BuildStatus::DONE;
        //TODO odoo.add_to_rebuild_odoo(Arc::downgrade(&self.sym_stack[0]));
    }

    fn eval_local_symbols_from_import_stmt(&self, odoo: &mut SyncOdoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, range: &TextRange) {
        if name_aliases.len() == 1 && name_aliases[0].name.to_string() == "*" {
            return;
        }
        let import_results: Vec<ImportResult> = resolve_import_stmt(
            odoo,
            self.sym_stack.last().unwrap(),
            self.sym_stack.last().unwrap(),
            from_stmt,
            name_aliases,
            level,
            range);
        
        for _import_result in import_results.iter() {
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&_import_result.name, &_import_result.range);
            if variable.is_none() {
                continue;
            }
            if _import_result.found {
                //resolve the symbol and build necessary evaluations
                let (mut _sym, mut instance): (Weak<RefCell<Symbol>>, bool) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, false);
                let mut old_ref: Option<Weak<RefCell<Symbol>>> = None;
                let mut arc_sym = _sym.upgrade().unwrap();
                let mut sym = arc_sym.borrow_mut();
                while sym.evaluation.is_none() && (old_ref.is_none() || !Rc::ptr_eq(&arc_sym, &old_ref.as_ref().unwrap().upgrade().unwrap())) {
                    old_ref = Some(_sym.clone());
                    let file_sym = sym.get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
                    drop(sym);
                    if file_sym.is_some() {
                        let arc_file_sym = file_sym.as_ref().unwrap().upgrade().unwrap();
                        if arc_file_sym.borrow_mut().arch_eval_status == BuildStatus::PENDING && odoo.is_in_rebuild(file_sym.as_ref().unwrap(), BuildSteps::ARCH_EVAL) {
                            let mut builder = PythonArchEval::new(arc_file_sym);
                            builder.eval_arch(odoo);
                            //TODO remove from list?
                            (_sym, instance) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, false);
                            arc_sym = _sym.upgrade().unwrap();
                            sym = arc_sym.borrow_mut();
                        } else {
                            sym = arc_sym.borrow_mut();
                        }
                    } else {
                        sym = arc_sym.borrow_mut();
                    }
                }
                drop(sym);
                if !Rc::ptr_eq(&arc_sym, &variable.as_ref().unwrap()) { //anti-loop. We want to be sure we are not evaluating to the same sym
                    variable.unwrap().borrow_mut().evaluation = Some(Evaluation::eval_from_symbol(&_import_result.symbol));
                    //TODO add dependency
                } else {
                    //TODO diagnostic
                }

            } else {
                //TODO add to not found symbols
            }
        }
    }

    fn _visit_ann_assign(&self, odoo: &mut SyncOdoo, ann_assign_stmt: &StmtAnnAssign) {
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

    fn _visit_assign(&self, odoo: &mut SyncOdoo, assign_stmt: &StmtAssign) {
        //TODO
    }
}