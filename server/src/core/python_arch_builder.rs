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
pub struct PythonArchBuilder {
    sym_stack: Vec<Arc<Mutex<Symbol>>>,
}

impl PythonArchBuilder {
    pub fn new(symbol: Arc<Mutex<Symbol>>) -> PythonArchBuilder {
        PythonArchBuilder {
            sym_stack: vec![symbol]
        }
    }

    pub async fn load_arch(&mut self, odoo: &mut Odoo) {
        println!("load arch");
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
        odoo.add_to_rebuild_arch_eval(Arc::downgrade(&self.sym_stack[0]));
    }

    fn create_local_symbols_from_import_stmt(&self, odoo: &mut Odoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, range: &TextRange) {
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
                if import_result.symbol.lock().unwrap().symbols.contains_key("__all__") {
                    // TODO implement __all__ imports
                }
                for s in import_result.symbol.lock().unwrap().symbols.values() {
                    let sub_symbol = s.lock().unwrap();
                    let mut variable = Symbol::new(sub_symbol.name.clone(), SymType::VARIABLE); //TODO mark as import
                    variable.range = Some(import_name.range.clone());
                    //TODO variable.eval = Evaluation(sub_symbol);
                    //TODO add dependency
                    self.sym_stack.last().unwrap().lock().unwrap().add_symbol(variable);
                }

            } else {
                let var_name = if import_name.asname.is_none() {
                    import_name.name.clone()
                } else {
                    import_name.asname.as_ref().unwrap().clone()
                };
                let mut variable = Symbol::new(var_name.to_string(), SymType::VARIABLE); //TODO mark as import
                variable.range = Some(import_name.range.clone());
                self.sym_stack.last().unwrap().lock().unwrap().add_symbol(variable);
            }
        }
    }
}