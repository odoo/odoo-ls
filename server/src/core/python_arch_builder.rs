use std::sync::{Arc, Mutex};

use rustpython_parser::ast::Stmt;

use crate::FILE_MGR;
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
                    println!("import stmt");
                },
                _ => {}
            }
        }
    }
}