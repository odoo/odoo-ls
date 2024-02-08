use std::sync::{Arc, Mutex};

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Stmt, Alias, Int};

use crate::constants::SymType;
use crate::FILE_MGR;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::Odoo;
use crate::core::symbol::Symbol;

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

    pub async fn eval_arch(&mut self, odoo: &mut Odoo) {
        println!("eval arch");
        let mut temp = FILE_MGR.lock().await;
        let symbol = self.sym_stack[0].lock().unwrap();
        if symbol.paths.len() != 1 {
            panic!()
        }
        let path = symbol.paths[0].clone();
        drop(symbol);
        let mut file_info = temp.get_file_info(path.as_str()); //create ast
        match file_info.ast {
            Some(_) => {},
            None => {
                file_info.build_ast(path.as_str(), "");
            }
        }
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
                //let ref = symbol.follow_ref()[0];
                //rebuild if necessary (WARNING: check dependencies before build eval?)
            } else {
                //TODO add to not found symbols
            }
        }
    }
}