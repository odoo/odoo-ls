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

pub struct ImportResult {
    pub name: String,
    pub found: bool,
    pub symbol: Arc<Mutex<Symbol>>,
    pub file_tree: Tree,
    pub range: TextRange,
}

pub fn resolve_import_stmt(odoo: &mut Odoo, source_file_symbol: &Arc<Mutex<Symbol>>, parent_symbol: &Arc<Mutex<Symbol>>, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, from_range: &TextRange) -> Vec<ImportResult> {
    //A: search base of different imports
    let file_tree = _resolve_packages(
        &source_file_symbol.lock().unwrap(),
        level,
        from_stmt);
    let (mut from_symbol, mut fallback_sym) = _get_or_create_symbol(
        odoo,
        odoo.symbols.as_ref().unwrap().clone(),
        &file_tree,
        source_file_symbol.lock().unwrap(),
        None,
        from_range);
    let mut result = vec![];
    for alias in name_aliases {
        result.push(ImportResult{
            name: alias.name.as_str().to_string().clone(),
            found: false,
            symbol: fallback_sym.clone(),
            file_tree: (file_tree.clone(), vec![]),
            range: alias.range.clone()
        })
    }
    if from_symbol.is_none() {
        return result;
    }

    let mut name_index: i32 = -1;
    for alias in name_aliases.iter() {
        let name = alias.name.as_str().to_string();
        name_index += 1;
        if name == "*" {
            result[name_index as usize].found = true;
            result[name_index as usize].symbol = from_symbol.as_ref().unwrap().clone();
            continue;
        }
        if alias.asname.is_none() {
            // If asname is not defined, we only search for the first part of the name.
            // In all "from X import A" case, it simply means search for A
            // But in "import A.B.C", it means search for A only.
            // If user typed import A.B.C as D, we will search for A.B.C to link it to symbol D,
            // but if user typed import A.B.C, we will only search for A and create A, as any use by after will require to type A.B.C
            let (mut name_symbol, fallback_sym) = _get_or_create_symbol(
                odoo,
                from_symbol.as_ref().unwrap().clone(),
                &name.split(".").map(str::to_string).collect(),
                source_file_symbol.lock().unwrap(),
                None,
                &alias.range);
            if name_symbol.is_none() {
                if !name.contains(".") {
                    name_symbol = from_symbol.as_ref().unwrap().lock().unwrap().get_symbol(vec![], &name.split(".").map(str::to_string).collect());
                }
                if name_symbol.is_none() {
                    result[name_index as usize].symbol = fallback_sym.clone();
                    continue;
                }
            }
            result[name_index as usize].found = true;
            result[name_index as usize].symbol = name_symbol.as_ref().unwrap().clone();
        }
        let name_split: Vec<String> = name.split(".").map(str::to_string).collect();
        let name_first_part: Vec<String> = Vec::from_iter(name_split[0..name_split.len()-1].iter().cloned());
        let name_last_name: Vec<String> = vec![name_split.last().unwrap().clone()];

        // get the full file_tree, including the first part of the name import stmt. (os in import os.path)
        let (mut next_symbol, fallback_sym) = _get_or_create_symbol(
            odoo,
            from_symbol.as_ref().unwrap().clone(),
            &name_first_part,
            source_file_symbol.lock().unwrap(),
            None,
            &alias.range);
        if next_symbol.is_none() {
            result[name_index as usize].symbol = fallback_sym.clone();
            continue;
        }
        // now we can search for the last symbol, or create it if it doesn't exist
        let (mut name_symbol, fallback_sym) = _get_or_create_symbol(
            odoo,
            next_symbol.as_ref().unwrap().clone(),
            &name_last_name,
            source_file_symbol.lock().unwrap(),
            None,
            &alias.range);
        if name_symbol.is_none() { //If not a file/package, try to look up in symbols in current file (second parameter of get_symbol)
            name_symbol = next_symbol.as_ref().unwrap().lock().unwrap().get_symbol(vec![], &name_last_name);
            if name_symbol.is_none() {
                result[name_index as usize].symbol = fallback_sym.clone();
                continue;
            }
        }
        // we found it ! store the result if not already done
        if result[name_index as usize].found == false {
            result[name_index as usize].found = true;
            result[name_index as usize].symbol = name_symbol.as_ref().unwrap().clone();
        }
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
            if lvl > tree.0.len() as u32 {
                println!("Level is too high and going out of scope");
                file_tree = vec![];
            } else {
                file_tree = Vec::from_iter(tree.0[0..tree.0.len()- lvl as usize].iter().cloned());
            }
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

fn _get_or_create_symbol(odoo: &mut Odoo, symbol: Arc<Mutex<Symbol>>, names: &Vec<String>, file_symbol: MutexGuard<Symbol>, asname: Option<String>, range: &TextRange) -> (Option<Arc<Mutex<Symbol>>>, Arc<Mutex<Symbol>>) {
    //TODO get arc from parent
    let mut sym: Option<Arc<Mutex<Symbol>>> = Some(symbol.clone());
    let mut last_symbol = symbol.clone();
    for branch in names {
        let mut next_symbol = symbol.lock().unwrap().get_symbol(vec![branch.clone()], &vec![]);
        if next_symbol.is_none() {
            next_symbol = match _resolve_new_symbol(odoo, symbol.clone(), &branch, asname.clone(), range) {
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

fn _resolve_new_symbol(odoo: &mut Odoo, parent: Arc<Mutex<Symbol>>, name: &String, asname: Option<String>, range: &TextRange) -> Result<Arc<Mutex<Symbol>>, String> {
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
    for path in _parent.paths.iter() {
        let mut full_path = Path::new(path.as_str()).join(name);
        if full_path.to_str().unwrap().to_string() == odoo.stubs_dir {
            full_path = full_path.join(name);
        }
        if is_dir_cs(full_path.to_str().unwrap().to_string()) {
            let symbol = Symbol::create_from_path(&full_path);
            let _arc_symbol = parent.lock().unwrap().add_symbol(symbol);
            odoo.add_to_rebuild_arch(Arc::downgrade(&_arc_symbol));
            return Ok(_arc_symbol);
        } else if is_file_cs(full_path.join(".py").to_str().unwrap().to_string()) {
            let symbol = Symbol::create_from_path(&full_path.join(".py"));
            let _arc_symbol = parent.lock().unwrap().add_symbol(symbol);
            odoo.add_to_rebuild_arch(Arc::downgrade(&_arc_symbol));
            return Ok(_arc_symbol);
        } else if is_file_cs(full_path.join(".pyi").to_str().unwrap().to_string()) {
            let symbol = Symbol::create_from_path(&full_path.join(".pyi"));
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
