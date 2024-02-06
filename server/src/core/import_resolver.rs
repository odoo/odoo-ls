use glob::glob;
use std::sync::{Arc, Mutex};
use std::path::Path;
use tower_lsp::lsp_types::Range;
use std::sync::MutexGuard;

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Alias, Int};
use crate::core::odoo::Odoo;
use crate::core::symbol::Symbol;
use crate::constants::*;
use crate::utils::{is_dir_cs, is_file_cs};

struct ImportResult {
    name: String,
    found: bool,
    symbol: Arc<Mutex<Symbol>>,
    file_tree: Tree,
    range: TextRange,
}

pub fn resolve_import_stmt(odoo: &Odoo, source_file_symbol: &Arc<Mutex<Symbol>>, parent_symbol: &Arc<Mutex<Symbol>>, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, from_range: &TextRange) -> Vec<ImportResult> {
    //A: search base of different imports
    let file_tree = _resolve_packages(
        &source_file_symbol.lock().unwrap(),
        level,
        from_stmt);
    let (from_symbol, fallback_sym) = _get_or_create_symbol(
        odoo,
        odoo.symbols.unwrap(),
        &file_tree,
        source_file_symbol.lock().unwrap(),
        None,
        from_range);
    let mut result = vec![];
    for alias in name_aliases {
        result.push(ImportResult{
            name: alias.name.as_str().to_string().clone(),
            found: false,
            symbol: fallback_sym,
            file_tree: (file_tree, vec![]),
            range: alias.range.clone()
        })
    }
    if from_symbol.is_none() {
        return result;
    }
    return result;
}

fn _find_module(odoo: &Odoo, name: String) -> Arc<Mutex<Symbol>> {
    todo!()
}

fn _resolve_packages(file_symbol: &MutexGuard<Symbol>, level: Option<&Int>, from_stmt: Option<&Identifier>) -> Vec<String> {
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
            file_tree = file_symbol.get_tree().0.clone();
        } else {
            let tree = file_symbol.get_tree();
            file_tree = Vec::from_iter(tree.0[0..tree.0.len()- lvl as usize].iter().cloned());
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

fn _get_or_create_symbol(odoo: &Odoo, symbol: Arc<Mutex<Symbol>>, names: &[String], file_symbol: MutexGuard<Symbol>, asname: Option<String>, range: &TextRange) -> (Option<Arc<Mutex<Symbol>>>, Arc<Mutex<Symbol>>) {
    //TODO get arc from parent
    let mut sym: Option<Arc<Mutex<Symbol>>> = Some(symbol.clone());
    let mut last_symbol = symbol.clone();
    for branch in names {
        let mut next_symbol = symbol.lock().unwrap().get_symbol(vec![branch.clone()], vec![]);
        if next_symbol.is_none() {
            next_symbol = match _resolve_new_symbol(odoo, symbol, branch, asname, range) {
                Ok(v) => Some(v),
                Err(e) => None
            }
        }
        if next_symbol.is_none() {
            sym = None;
            break;
        }
        last_symbol = next_symbol.unwrap().clone();
    }
    return (sym, last_symbol)
}

fn _resolve_new_symbol(odoo: &Odoo, parent: Arc<Mutex<Symbol>>, name: &String, asname: Option<String>, range: &TextRange) -> Result<Arc<Mutex<Symbol>>, String> {
    let _parent = parent.lock().unwrap();
    let sym_name: String = match asname {
        Some(asname_inner) => asname_inner.clone(),
        None => name.clone()
    };
    if _parent.sym_type == SymType::COMPILED {
        let mut compiled_sym = Symbol::new(sym_name, SymType::COMPILED);
        compiled_sym.range = Some(range.clone());
        return Ok(parent.lock().unwrap().add_symbol(compiled_sym));
    }
    for path in _parent.paths {
        let mut full_path = Path::new(path.as_str()).join(name);
        if full_path.to_str().unwrap().to_string() == odoo.stubs_dir {
            full_path = full_path.join(name);
        }
        if is_dir_cs(full_path.to_str().unwrap().to_string()) {
            let symbol = Symbol::create_from_path(full_path.as_os_str().to_str().unwrap());
            let _arc_symbol = parent.lock().unwrap().add_symbol(symbol);
            odoo.add_to_rebuild_arch(Arc::downgrade(&_arc_symbol));
            return Ok(_arc_symbol);
        } else if is_file_cs(full_path.join(".py").to_str().unwrap().to_string()) {
            let symbol = Symbol::create_from_path(full_path.join(".py").as_os_str().to_str().unwrap());
            let _arc_symbol = parent.lock().unwrap().add_symbol(symbol);
            odoo.add_to_rebuild_arch(Arc::downgrade(&_arc_symbol));
            return Ok(_arc_symbol);
        } else if is_file_cs(full_path.join(".pyi").to_str().unwrap().to_string()) {
            let symbol = Symbol::create_from_path(full_path.join(".pyi").as_os_str().to_str().unwrap());
            let _arc_symbol = parent.lock().unwrap().add_symbol(symbol);
            odoo.add_to_rebuild_arch(Arc::downgrade(&_arc_symbol));
            return Ok(_arc_symbol);
        } else if !parent.lock().unwrap().get_tree().0.is_empty() {
            if cfg!(windows) {
                for entry in glob((full_path.as_os_str().to_str().unwrap().to_owned() + "*.pyd").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(path) => {
                            let compiled_sym = Symbol::new(sym_name, SymType::COMPILED);
                            return Ok(parent.lock().unwrap().add_symbol(compiled_sym));
                        }
                        Err(e) => {},
                    }
                }
            } else if cfg!(linux) {
                for entry in glob((full_path.as_os_str().to_str().unwrap().to_owned() + "*.so").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(path) => {
                            let compiled_sym = Symbol::new(sym_name, SymType::COMPILED);
                            return Ok(parent.lock().unwrap().add_symbol(compiled_sym));
                        }
                        Err(e) => {},
                    }
                }
            }
        }
    }
    return Err("Symbol not found".to_string())
}
