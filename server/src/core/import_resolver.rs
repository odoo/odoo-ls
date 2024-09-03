use glob::glob;
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range};
use tracing::error;
use std::collections::HashSet;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::Path;

use ruff_text_size::TextRange;
use ruff_python_ast::{Alias, Identifier};
use crate::{constants::*, S};
use crate::threads::SessionInfo;
use crate::utils::{is_dir_cs, is_file_cs, PathSanitizer};

use super::odoo::SyncOdoo;
use super::symbols::symbol::Symbol;

pub struct ImportResult {
    pub name: String,
    pub found: bool,
    pub symbol: Rc<RefCell<Symbol>>,
    pub file_tree: Tree,
    pub range: TextRange,
}

fn resolve_import_stmt_hook(alias: &Alias, from_symbol: &Option<Rc<RefCell<Symbol>>>, session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt: Option<&Identifier>, level: Option<u32>, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Option<ImportResult>{
    if session.sync_odoo.version_major >= 17 && alias.name.as_str() == "Form" && (*(from_symbol.as_ref().unwrap())).borrow().get_tree().0 == vec!["odoo", "tests", "common"]{
        let mut results = resolve_import_stmt(session, source_file_symbol, Some(&Identifier::new(S!("odoo.tests"), from_stmt.unwrap().range)), &[alias.clone()], level, &mut None);
        if let Some(diagnostic) = diagnostics.as_mut() {
            diagnostic.push(
                Diagnostic::new(
                        Range::new(Position::new(alias.range.start().to_u32(), 0), Position::new(alias.range.end().to_u32(), 0)),
                        Some(DiagnosticSeverity::WARNING),
                            Some(NumberOrString::String(S!("OLS20006"))),
                            Some(EXTENSION_NAME.to_string()),
                            S!("Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form"),
                            None,
                        Some(vec![DiagnosticTag::DEPRECATED]),
                )
            );
        }
        results.pop()
    } else {
        None
    }
}

pub fn resolve_import_stmt(session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Vec<ImportResult> {
    //A: search base of different imports
    let _source_file_symbol_lock = source_file_symbol.borrow_mut();
    let file_tree = _resolve_packages(
        &_source_file_symbol_lock.paths()[0].clone(),
        &_source_file_symbol_lock.get_tree(),
        &_source_file_symbol_lock.typ(),
        level,
        from_stmt);
    drop(_source_file_symbol_lock);
    let (from_symbol, fallback_sym) = _get_or_create_symbol(
        session,
        session.sync_odoo.symbols.as_ref().unwrap().clone(),
        &file_tree,
        None);
    let mut result = vec![];
    for alias in name_aliases {
        result.push(ImportResult{
            name: alias.asname.as_ref().unwrap_or(&alias.name).to_string().clone(),
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
        if let Some(hook_result) = resolve_import_stmt_hook(alias, &from_symbol, session, source_file_symbol, from_stmt, level,  diagnostics){
            result[name_index as usize] = hook_result;
            continue;
        }
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
                session,
                from_symbol.as_ref().unwrap().clone(),
                &vec![name.split(".").map(str::to_string).next().unwrap()],
                None);
            if name_symbol.is_none() {
                if !name.contains(".") {
                    //TODO WTF?
                    let name_symbol_vec = from_symbol.as_ref().unwrap().borrow_mut().get_symbol(&(vec![], vec![name.clone()]), u32::MAX);
                    //TODO what if multiple values?
                    name_symbol = name_symbol_vec.get(0).cloned();
                }
                if name_symbol.is_none() {
                    result[name_index as usize].symbol = fallback_sym.clone();
                    continue;
                }
            }
            result[name_index as usize].name = name.split(".").map(str::to_string).next().unwrap();
            result[name_index as usize].found = true;
            result[name_index as usize].symbol = name_symbol.as_ref().unwrap().clone();
            continue;
        }
        let name_split: Vec<String> = name.split(".").map(str::to_string).collect();
        let name_first_part: Vec<String> = Vec::from_iter(name_split[0..name_split.len()-1].iter().cloned());
        let name_last_name: Vec<String> = vec![name_split.last().unwrap().clone()];

        // get the full file_tree, including the first part of the name import stmt. (os in import os.path)
        let (next_symbol, fallback_sym) = _get_or_create_symbol(
            session,
            from_symbol.as_ref().unwrap().clone(),
            &name_first_part,
            None);
        if next_symbol.is_none() {
            result[name_index as usize].symbol = fallback_sym.clone();
            continue;
        }
        // now we can search for the last symbol, or create it if it doesn't exist
        let (mut name_symbol, fallback_sym) = _get_or_create_symbol(
            session,
            next_symbol.as_ref().unwrap().clone(),
            &name_last_name,
            None);
        if name_symbol.is_none() { //If not a file/package, try to look up in symbols in current file (second parameter of get_symbol)
            //TODO what if multiple values?
            name_symbol = next_symbol.as_ref().unwrap().borrow_mut().get_symbol(&(vec![], name_last_name), u32::MAX).get(0).cloned();
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

pub fn find_module(session: &mut SessionInfo, odoo_addons: Rc<RefCell<Symbol>>, name: &String) -> Option<Rc<RefCell<Symbol>>> {
    let paths = (*odoo_addons).borrow().paths().clone();
    for path in paths.iter() {
        let full_path = Path::new(path.as_str()).join(name);
        if is_dir_cs(full_path.sanitize()) {
            let _arc_symbol = Symbol::create_from_path(session, &full_path, odoo_addons.clone(), false);
            if _arc_symbol.is_some() {
                let typ = _arc_symbol.as_ref().unwrap().borrow().typ();
                match typ {
                    SymType::NAMESPACE => {
                        return Some(_arc_symbol.as_ref().unwrap().clone());
                    },
                    SymType::PACKAGE => {
                        let _arc_symbol = _arc_symbol.as_ref().unwrap().clone();
                        session.sync_odoo.modules.insert(name.clone(), Rc::downgrade(&_arc_symbol));
                        session.sync_odoo.add_to_rebuild_arch(_arc_symbol.clone());
                        return Some(_arc_symbol);
                    },
                    _ => {return None}
                }
            }
        }
    }
    None
}

fn _resolve_packages(file_path: &String, file_tree: &Tree, file_sym_type: &SymType, level: Option<u32>, from_stmt: Option<&Identifier>) -> Vec<String> {
    let mut first_part_tree: Vec<String> = vec![];
    if level.is_some() && level.unwrap() > 0 {
        let mut lvl = level.unwrap();
        if lvl > Path::new(file_path).components().count() as u32 {
            panic!("Level is too high!")
        }
        if *file_sym_type == SymType::PACKAGE {
            lvl -= 1;
        }
        if lvl == 0 {
            first_part_tree = file_tree.0.clone();
        } else {
            let tree = file_tree;
            if lvl > tree.0.len() as u32 {
                error!("Level is too high and going out of scope");
                first_part_tree = vec![];
            } else {
                first_part_tree = Vec::from_iter(tree.0[0..tree.0.len()- lvl as usize].iter().cloned());
            }
        }
    }
    match from_stmt {
        Some(from_stmt_inner) => {
            let split = from_stmt_inner.as_str().split(".");
            for i in split {
                first_part_tree.push(i.to_string());
            }
        },
        None => ()
    }
    first_part_tree
}

fn _get_or_create_symbol(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>, names: &Vec<String>, asname: Option<String>) -> (Option<Rc<RefCell<Symbol>>>, Rc<RefCell<Symbol>>) {
    let mut sym: Option<Rc<RefCell<Symbol>>> = Some(symbol.clone());
    let mut last_symbol = symbol.clone();
    for branch in names.iter() {
        let mut next_symbol = sym.as_ref().unwrap().borrow_mut().get_symbol(&(vec![branch.clone()], vec![]), u32::MAX);
        if next_symbol.is_empty() {
            next_symbol = match _resolve_new_symbol(session, sym.as_ref().unwrap().clone(), &branch, asname.clone()) {
                Ok(v) => vec![v],
                Err(_) => vec![]
            }
        }
        if next_symbol.is_empty() {
            sym = None;
            break;
        }
        sym = Some(next_symbol[0].clone());
        last_symbol = next_symbol[0].clone();
    }
    return (sym, last_symbol)
}

fn _resolve_new_symbol(session: &mut SessionInfo, parent: Rc<RefCell<Symbol>>, name: &String, asname: Option<String>) -> Result<Rc<RefCell<Symbol>>, String> {
    let sym_name: String = match asname {
        Some(asname_inner) => asname_inner.clone(),
        None => name.clone()
    };
    if (*parent).borrow().typ() == SymType::COMPILED {
        return Ok((*parent).borrow_mut().add_new_compiled(session, &sym_name, &S!("")));
    }
    let paths = (*parent).borrow().paths().clone();
    for path in paths.iter() {
        let mut full_path = Path::new(path.as_str()).join(name);
        for stub in session.sync_odoo.stubs_dirs.iter() {
            if path.as_str().to_string() == *stub {
                full_path = full_path.join(name);
            }
        }
        if is_dir_cs(full_path.sanitize()) {
            // if is_dir_cs(full_path.to_str().unwrap().to_string() + "-stubs") {
            //     full_path.set_file_name(full_path.file_name().unwrap().to_str().unwrap().to_string() + "-stubs");
            // }
            let _rc_symbol = Symbol::create_from_path(session, &full_path, parent.clone(), false);
            if _rc_symbol.is_some() {
                let _arc_symbol = _rc_symbol.unwrap();
                SyncOdoo::rebuild_arch_now(session, &_arc_symbol);
                return Ok(_arc_symbol);
            }
        } else if is_file_cs(full_path.with_extension("py").sanitize()) {
            let _arc_symbol = Symbol::create_from_path(session, &full_path.with_extension("py"), parent.clone(), false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                SyncOdoo::rebuild_arch_now(session, &_arc_symbol);
                return Ok(_arc_symbol);
            }
        } else if is_file_cs(full_path.with_extension("pyi").sanitize()) {
            let _arc_symbol = Symbol::create_from_path(session, &full_path.with_extension("pyi"), parent.clone(), false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                SyncOdoo::rebuild_arch_now(session, &_arc_symbol);
                return Ok(_arc_symbol);
            }
        } else if !(*parent).borrow().get_tree().0.is_empty() {
            if cfg!(windows) {
                for entry in glob((full_path.sanitize() + "*.pyd").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(_path) => {
                            return Ok((*parent).borrow_mut().add_new_compiled(session, &sym_name, &_path.to_str().unwrap().to_string()));
                        }
                        Err(_) => {},
                    }
                }
            } else if cfg!(linux) {
                for entry in glob((full_path.sanitize() + "*.so").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(_path) => {
                            return Ok((*parent).borrow_mut().add_new_compiled(session, &sym_name, &_path.to_str().unwrap().to_string()));
                        }
                        Err(_) => {},
                    }
                }
            }
        }
    }
    return Err("Symbol not found".to_string())
}

pub fn get_all_valid_names(session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt: Option<&Identifier>, base_name: String, level: Option<u32>) -> HashSet<String> {
    //A: search base of different imports
    let _source_file_symbol_lock = source_file_symbol.borrow_mut();
    let file_tree = _resolve_packages(
        &_source_file_symbol_lock.paths()[0].clone(),
        &_source_file_symbol_lock.get_tree(),
        &_source_file_symbol_lock.typ(),
        level,
        from_stmt);
    drop(_source_file_symbol_lock);
    let (from_symbol, _fallback_sym) = _get_or_create_symbol(
        session,
        session.sync_odoo.symbols.as_ref().unwrap().clone(),
        &file_tree,
        None);
    let mut result = HashSet::new();
    if from_symbol.is_none() {
        return result;
    }
    let from_symbol = from_symbol.unwrap();

    let mut sym: Option<Rc<RefCell<Symbol>>> = Some(from_symbol.clone());
    let mut names = vec![base_name.split(".").map(str::to_string).next().unwrap()];
    if base_name.ends_with(".") {
        names.push(S!(""));
    }
    for (index, branch) in names.iter().enumerate() {
        if index != names.len() -1 {
            let mut next_symbol = sym.as_ref().unwrap().borrow_mut().get_symbol(&(vec![branch.clone()], vec![]), u32::MAX);
            if next_symbol.is_empty() {
                next_symbol = match _resolve_new_symbol(session, sym.as_ref().unwrap().clone(), &branch, None) {
                    Ok(v) => vec![v],
                    Err(_) => vec![]
                }
            }
            if next_symbol.is_empty() {
                sym = None;
                break;
            }
            sym = Some(next_symbol[0].clone());
        }
    }
    if let Some(sym) = sym {
        let filter = names.last().unwrap();
        for symbol in sym.borrow().all_symbols() {
            if symbol.borrow().name().starts_with(filter) {
                result.insert(symbol.borrow().name().clone());
            }
        }
    }

    return result;
}
