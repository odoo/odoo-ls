use std::sync::{Arc, Mutex};
use std::path::Path;
use tower_lsp::lsp_types::Range;

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Alias, Int};
use crate::core::odoo::Odoo;
use crate::core::symbol::Symbol;
use crate::constants::SymType;

pub fn resolve_import_stmt(odoo: &Odoo, source_file_symbol: &Symbol, parent_symbol: &Symbol, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>) -> &Symbol {
    let file_tree = _resolve_packages(source_file_symbol, level, from_stmt);
    todo!("please finish this function")
}

fn _resolve_packages(file_symbol: &Symbol, level: Option<&Int>, from_stmt: Option<&Identifier>) -> Vec<String> {
    let mut file_tree: Vec<String> = vec![];
    if level.is_some() && level.unwrap().to_u32() > 0 {
        let mut lvl = level.unwrap().to_u32();
        if lvl > Path::new(file_symbol.paths[0].as_str()).components().count() as u32 {
            panic!("Level is too high!")
        }
        if file_symbol.sym_type == SymType::PACKAGE {
            lvl -= 1;
        }
        if lvl == 0 {
            file_tree = file_symbol.get_tree()[0].clone();
        } else {
            let tree = file_symbol.get_tree();
            file_tree = Vec::from_iter(tree[0][0..tree[0].len()- lvl as usize].iter().cloned());
        }
    }
    match from_stmt {
        Some(from_stmt_inner) => {
            let split = from_stmt_inner.as_str().split(".");
            for i in split {
                file_tree.push(i.to_string());
            }
        },
        None => ()
    }
    file_tree
}

fn _get_or_create_symbol(odoo: &Odoo, symbol: Arc<Mutex<Symbol>>, names: &[String], file_symbol: &Symbol, asname: Option<String>) -> (Arc<Mutex<Symbol>>, Arc<Mutex<Symbol>>) {
    //TODO get arc from parent
    let mut sym: Option<Arc<Mutex<Symbol>>> = Some(symbol.clone());
    let mut last_symbol = symbol.clone();
    for branch in names {
        let mut next_symbol = symbol.lock().unwrap().get_symbol(vec![branch.clone()], vec![]);
        if next_symbol.is_none() {
            next_symbol = _resolve_new_symbol(odoo, ???);
        }
        if next_symbol.is_none() {
            sym = None;
            break;
        }
        last_symbol = next_symbol.unwrap().clone();
    }
    return (symbol, last_symbol)
}

fn _resolve_new_symbol(odoo: &Odoo, parent: Arc<Mutex<Symbol>>, name: &String, asname: Option<String>, range: &Range) {
    let _parent = parent.lock().unwrap();
    if _parent.sym_type == SymType::COMPILED {
        //TODO create compiledSymbol
    }
    for path in _parent.paths {
        let mut full_path = Path::new(path.as_str()).join(name);
        if full_path.to_str().unwrap().to_string() == odoo.stubs_dir {
            full_path = full_path.join(name);
        }
        match Symbol::create_from_path(full_path, &Some(parent)).await {
            Some(symbol) => {
                return symbol;
            },
            None => {
                //TODO create compiledSymbol
            }
        }
    }
}
