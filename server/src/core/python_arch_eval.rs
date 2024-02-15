use std::sync::{Arc, Mutex, Weak};

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Stmt, Alias, Int};
use std::path::PathBuf;

use crate::constants::*;
use crate::FILE_MGR;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::Odoo;
use crate::core::symbol::Symbol;
use crate::core::evaluation::Evaluation;

use super::import_resolver::ImportResult;


#[derive(Debug, Clone)]
pub struct PythonArchEval {
    sym_stack: Vec<Arc<Mutex<Symbol>>>,
}

impl PythonArchEval {
    pub fn new(symbol: Arc<Mutex<Symbol>>) -> PythonArchEval {
        PythonArchEval {
            sym_stack: vec![symbol]
        }
    }

    pub fn eval_arch(&mut self, odoo: &mut Odoo) {
        println!("eval arch");
        let mut file_mgr = FILE_MGR.lock().unwrap();
        let symbol = self.sym_stack[0].lock().unwrap();
        if symbol.paths.len() != 1 {
            panic!()
        }
        let mut path = symbol.paths[0].clone();
        if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + symbol.i_ext.as_str();
        }
        drop(symbol);
        let mut file_info = file_mgr.get_file_info(path.as_str()); //create ast
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
                        self.eval_local_symbols_from_import_stmt(odoo, None, &import_stmt.names, None, &import_stmt.range)
                    },
                    Stmt::ImportFrom(import_from_stmt) => {
                        self.eval_local_symbols_from_import_stmt(odoo, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level.as_ref(), &import_from_stmt.range)
                    },
                    _ => {}
                }
            }
        }
        //TODO odoo.add_to_rebuild_arch_eval(Arc::downgrade(&self.sym_stack[0]));
    }

    fn eval_local_symbols_from_import_stmt(&self, odoo: &mut Odoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, range: &TextRange) {
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
            let variable = self.sym_stack.last().unwrap().lock().unwrap().get_positioned_symbol(&_import_result.name, &_import_result.range);
            if variable.is_none() {
                continue;
            }
            if _import_result.found {
                //resolve the symbol and build necessary evaluations
                let (mut _sym, mut instance): (Weak<Mutex<Symbol>>, bool) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, false);
                let mut old_ref: Option<Weak<Mutex<Symbol>>> = None;
                let mut arc_sym = _sym.upgrade().unwrap();
                let mut sym = arc_sym.lock().unwrap();
                while sym.evaluation.is_none() && (old_ref.is_none() || Arc::ptr_eq(&arc_sym, &old_ref.as_ref().unwrap().upgrade().unwrap())) {
                    old_ref = Some(_sym.clone());
                    let file_sym = sym.get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
                    if file_sym.is_some() {
                        let arc_file_sym = file_sym.as_ref().unwrap().upgrade().unwrap();
                        if arc_file_sym.lock().unwrap().arch_eval_status == false && odoo.is_in_rebuild(file_sym.as_ref().unwrap(), BuildSteps::ARCH_EVAL) {
                            let mut builder = PythonArchEval::new(arc_file_sym);
                            builder.eval_arch(odoo);
                            //TODO remove from list?
                            (_sym, instance) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, false);
                            drop(sym);
                            arc_sym = _sym.upgrade().unwrap();
                            sym = arc_sym.lock().unwrap();
                        }
                    }
                }
                if !Arc::ptr_eq(&arc_sym, &variable.as_ref().unwrap()) { //anti-loop
                    variable.unwrap().lock().unwrap().evaluation = Some(Evaluation::eval_from_symbol(&_import_result.symbol));
                    //TODO add dependency
                } else {
                    //TODO diagnostic
                }

            } else {
                //TODO add to not found symbols
            }
        }
    }
}