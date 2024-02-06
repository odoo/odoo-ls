use std::sync::{Arc, Mutex};

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Stmt, Alias, Int};

use crate::FILE_MGR;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::Odoo;
use crate::core::symbol::Symbol;


#[derive(Debug, Clone)]
pub struct PythonArchBuilder {
    sym_stack: Vec<Arc<Mutex<Symbol>>>,
}

impl PythonArchBuilder {
    pub fn new(symbol: Arc<Mutex<Symbol>>) -> PythonArchBuilder {
        PythonArchBuilder {
            sym_stack: vec![symbol]
        }
    }

    pub async fn load_arch(&mut self, odoo: &Odoo) {
        let mut temp = FILE_MGR.lock().await;
        let symbol = self.sym_stack[0].lock().unwrap();
        if symbol.paths.len() != 1 {
            panic!()
        }
        let path = symbol.paths[0].clone();
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
                    println!("{:?}", import_stmt);
                    self.create_local_symbols_from_import_stmt(odoo, None, &import_stmt.names, None, &import_stmt.range)
                },
                Stmt::ImportFrom(import_from_stmt) => {
                    println!("{:?}", import_from_stmt);
                    self.create_local_symbols_from_import_stmt(odoo, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level.as_ref(), &import_from_stmt.range)
                },
                _ => {}
            }
        }
    }

    fn create_local_symbols_from_import_stmt(&self, odoo: &Odoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, range: &TextRange) {
        for import_name in name_aliases {
            if import_name.name.as_str() == "*" {
                if self.sym_stack.len() != 1 { //only at top level for now.
                    continue;
                }
                let symbols = resolve_import_stmt(
                    odoo,
                    self.sym_stack.last().unwrap(),
                    self.sym_stack.last().unwrap(),
                    from_stmt,
                    name_aliases,
                    level,
                    range);
            } else {

            }
        }
    }
}