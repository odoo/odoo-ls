use glob::glob;
use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::name::Name;
use tracing::error;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use ruff_text_size::{TextRange, TextSize};
use ruff_python_ast::{Alias, AtomicNodeIndex, Identifier};
use crate::{constants::*, oyarn, Sy, S};
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::threads::SessionInfo;
use crate::utils::{is_dir_cs, is_file_cs, PathSanitizer};

use super::entry_point::{EntryPoint, EntryPointType};
use super::odoo::SyncOdoo;
use super::symbols::symbol::Symbol;

pub struct ImportResult {
    pub name: OYarn,
    pub found: bool,
    pub symbol: Rc<RefCell<Symbol>>,
    pub file_tree: Tree,
    pub range: TextRange,
}

//class used to cache import results in a build execution to speed up subsequent imports. 
//It means of course than a modification during the build will not be taken into account, but it should be ok because reloaded after the build
#[derive(Debug)]
pub struct ImportCache {
    pub modules: HashMap<OYarn, Option<Rc<RefCell<Symbol>>>>,
    pub main_modules: HashMap<OYarn, Option<Rc<RefCell<Symbol>>>>,
}

fn resolve_import_stmt_hook(alias: &Alias, from_symbol: &Option<Rc<RefCell<Symbol>>>, session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt: Option<&Identifier>, level: Option<u32>, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Option<ImportResult>{
    if session.sync_odoo.version_major >= 17 && alias.name.as_str() == "Form" && (*(from_symbol.as_ref().unwrap())).borrow().get_main_entry_tree(session).0 == vec!["odoo", "tests", "common"]{
        let mut results = resolve_import_stmt(session, source_file_symbol, Some(&Identifier::new(S!("odoo.tests"), from_stmt.unwrap().range)), &[alias.clone()], level, &mut None);
        if let Some(diagnostic) = diagnostics.as_mut() {
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03301, &[]) {
                diagnostic.push(Diagnostic {
                    range: Range::new(Position::new(alias.range.start().to_u32(), 0), Position::new(alias.range.end().to_u32(), 0)),
                    tags: Some(vec![DiagnosticTag::DEPRECATED]),
                    ..diagnostic_base
                });
            }
        }
        results.pop()
    } else {
        None
    }
}

/**
 * Helper to manually import a symbol. Do not forget to use level instead of '.' in the from_stmt parameter.
 */
pub fn manual_import(session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt:Option<String>, name: &str, asname: Option<String>, level: Option<u32>, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Vec<ImportResult> {
    let name_aliases = vec![Alias {
        name: Identifier { id: Name::new(name), range: TextRange::new(TextSize::new(0), TextSize::new(0)), node_index: AtomicNodeIndex::dummy() },
        asname: match asname {
            Some(asname_inner) => Some(Identifier { id: Name::new(asname_inner), range: TextRange::new(TextSize::new(0), TextSize::new(0)), node_index: AtomicNodeIndex::dummy() }),
            None => None,
        },
        range: TextRange::new(TextSize::new(0), TextSize::new(0)),
        node_index: AtomicNodeIndex::dummy()
    }];
    let from_stmt = match from_stmt {
        Some(from_stmt_inner) => Some(Identifier { id: Name::new(from_stmt_inner), range: TextRange::new(TextSize::new(0), TextSize::new(0)), node_index: AtomicNodeIndex::dummy() }),
        None => None,
    };
    resolve_import_stmt(session, source_file_symbol, from_stmt.as_ref(), &name_aliases, level, diagnostics)
}

pub fn resolve_import_stmt(session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Vec<ImportResult> {
    //A: search base of different imports
    let source_root = source_file_symbol.borrow().get_root().as_ref().unwrap().upgrade().unwrap();
    let entry = source_root.borrow().get_entry().unwrap();
    let _source_file_symbol_lock = source_file_symbol.borrow_mut();
    let file_tree = _resolve_packages(
        &_source_file_symbol_lock,
        level,
        from_stmt);
    drop(_source_file_symbol_lock);
    let source_path = source_file_symbol.borrow().paths()[0].clone();
    let mut start_symbol = None;
    if level.is_some() && level.unwrap() != 0 {
        //if level is some, resolve_packages already built a full tree, so we can start from root
        start_symbol = Some(source_root.clone());
    }
    let (from_symbol, fallback_sym) = _get_or_create_symbol(
        session,
        &entry,
        source_path.as_str(),
        start_symbol,
        &file_tree,
    None,
        level);
    let mut result = vec![];
    for alias in name_aliases {
        result.push(ImportResult{
            name: OYarn::from(alias.asname.as_ref().unwrap_or(&alias.name).to_string()),
            found: false,
            symbol: fallback_sym.as_ref().unwrap_or(&source_root).clone(),
            file_tree: (file_tree.clone(), vec![]),
            range: alias.range.clone()
        })
    }
    if from_symbol.is_none() && level.is_some() {
        return result;
    }

    let mut name_index: i32 = -1;
    for alias in name_aliases.iter() {
        let name = oyarn!("{}", alias.name);
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
        let name_split: Vec<OYarn> = name.split(".").map(|s| oyarn!("{}", s)).collect();
        let name_first_part: Vec<OYarn> = vec![name_split.first().unwrap().clone()];
        let name_middle_part: Vec<OYarn> = if name_split.len() > 2 {
            Vec::from_iter(name_split[1..name_split.len()-1].iter().cloned())
        } else {
            vec![]
        };
        let name_last_name: Vec<OYarn> = vec![name_split.last().unwrap().clone()];
        let (mut next_symbol, mut fallback_sym) = _get_or_create_symbol(
            session,
            &entry,
            source_path.as_str(),
            from_symbol.clone(),
            &name_first_part,
            None,
        None);
        if next_symbol.is_none() && name_split.len() == 1 && from_symbol.is_some() {
            //check the last name is not a symbol in the file
            let name_symbol_vec = from_symbol.as_ref().unwrap().borrow().get_symbol(&(vec![], name_first_part), u32::MAX);
            next_symbol = name_symbol_vec.last().cloned();
        }
        if next_symbol.is_none() {
            result[name_index as usize].symbol = fallback_sym.as_ref().unwrap_or(&source_root).clone();
            continue;
        }
        if alias.asname.is_none() {
            // If asname is not defined, we only have to return the first symbol found. However we have to search for the other names to import them too
            // In all "from X import A" case, it simply means search for A
            // But in "import A.B.C", it means search for A only, and import B.C
            // If user typed import A.B.C as D, we will search for A.B.C to link it to symbol D,
            result[name_index as usize].name = name.split(".").map(|s| oyarn!("{}", s)).next().unwrap();
            result[name_index as usize].found = true;
            result[name_index as usize].symbol = next_symbol.as_ref().unwrap().clone();
        }
        if !name_middle_part.is_empty() {
            (next_symbol, fallback_sym) = _get_or_create_symbol(
                session,
                &entry,
                "",
                Some(next_symbol.as_ref().unwrap().clone()),
                &name_middle_part,
                None,
            None);
        }
        if next_symbol.is_none() {
            if alias.asname.is_some() {
                result[name_index as usize].symbol = fallback_sym.as_ref().unwrap_or(&source_root).clone();
            }
            continue;
        }
        if name_split.len() > 1 {
            // now we can search for the last symbol, or create it if it doesn't exist
            let (mut last_symbol, fallback_sym) = _get_or_create_symbol(
                session,
                &entry,
                "",
                Some(next_symbol.as_ref().unwrap().clone()),
                &name_last_name,
                None,
            None);
            if last_symbol.is_none() { //If not a file/package, try to look up in symbols in current file (second parameter of get_symbol)
                //TODO what if multiple values?
                let ns = next_symbol.as_ref().unwrap().borrow().get_symbol(&(vec![], name_last_name), u32::MAX).get(0).cloned();
                last_symbol = ns;
                if alias.asname.is_some() && last_symbol.is_none() {
                    result[name_index as usize].symbol = fallback_sym.as_ref().unwrap_or(&source_root).clone();
                    continue;
                }
            }
            // we found it ! store the result if not already done
            if alias.asname.is_some() && result[name_index as usize].found == false {
                result[name_index as usize].found = true;
                result[name_index as usize].symbol = last_symbol.as_ref().unwrap().clone();
            }
        } else {
            //everything is ok, let's store the result if not already done
            result[name_index as usize].name = name.split(".").map(|s| oyarn!("{}", s)).next().unwrap();
            result[name_index as usize].found = true;
            result[name_index as usize].symbol = next_symbol.as_ref().unwrap().clone();
        }
    }

    return result;
}

pub fn find_module(session: &mut SessionInfo, odoo_addons: Rc<RefCell<Symbol>>, name: &OYarn) -> Option<Rc<RefCell<Symbol>>> {
    let paths = (*odoo_addons).borrow().paths().clone();
    for path in paths.iter() {
        let full_path = Path::new(path.as_str()).join(name.as_str());
        if !is_dir_cs(full_path.sanitize()) {
            continue;
        }
        let Some(module_symbol) = Symbol::create_from_path(session, &full_path, odoo_addons.clone(), true) else {
            continue;
        };
        session.sync_odoo.modules.insert(name.clone(), Rc::downgrade(&module_symbol));
        SyncOdoo::build_now(session, &module_symbol, BuildSteps::ARCH);
        return Some(module_symbol.clone());
    }
    None
}

fn _resolve_packages(from_file: &Symbol, level: Option<u32>, from_stmt: Option<&Identifier>) -> Vec<OYarn> {
    let mut first_part_tree: Vec<OYarn> = vec![];
    if level.is_some() && level.unwrap() > 0 {
        let mut lvl = level.unwrap();
        if lvl > Path::new(&from_file.paths()[0]).components().count() as u32 {
            panic!("Level is too high!")
        }
        if matches!(from_file.typ(), SymType::PACKAGE(_)) {
            lvl -= 1;
        }
        if lvl == 0 {
            first_part_tree = from_file.get_tree().0.clone();
        } else {
            let tree = from_file.get_tree();
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
                first_part_tree.push(oyarn!("{}", i));
            }
        },
        None => ()
    }
    first_part_tree
}

fn _get_or_create_symbol(session: &mut SessionInfo, for_entry: &Rc<RefCell<EntryPoint>>, from_path: &str, symbol: Option<Rc<RefCell<Symbol>>>, names: &Vec<OYarn>, asname: Option<String>, level: Option<u32>) -> (Option<Rc<RefCell<Symbol>>>, Option<Rc<RefCell<Symbol>>>) {
    let mut sym: Option<Rc<RefCell<Symbol>>> = symbol.clone();
    let mut last_symbol = symbol.clone();
    for branch in names.iter() {
        match sym {
            Some(ref s) => {
                let mut next_symbol = s.borrow().get_symbol(&(vec![branch.clone()], vec![]), u32::MAX);
                if next_symbol.is_empty() && matches!(s.borrow().typ(), SymType::ROOT | SymType::NAMESPACE | SymType::PACKAGE(_) | SymType::COMPILED | SymType::DISK_DIR) {
                    next_symbol = match _resolve_new_symbol(session, s.clone(), &branch, asname.clone()) {
                        Ok(v) => vec![v],
                        Err(_) => vec![]
                    }
                }
                if next_symbol.is_empty() {
                    sym = None;
                    break;
                }
                sym = Some(next_symbol[0].clone());
                last_symbol = Some(next_symbol[0].clone());
            },
            None => {
                if level.is_none() || level.unwrap() == 0 {
                    if let Some(ref cache) = session.sync_odoo.import_cache {
                        let cache_module = if for_entry.borrow().typ == EntryPointType::MAIN || for_entry.borrow().typ == EntryPointType::ADDON {
                            cache.main_modules.get(branch)
                        } else {
                            cache.modules.get(branch)
                        };
                        if let Some(symbol) = cache_module {
                            if let Some(symbol) = symbol {
                                sym = Some(symbol.clone());
                                last_symbol = Some(symbol.clone());
                                continue;
                            } else{
                                //we know we won't find it
                                sym = None;
                                last_symbol = None;
                                break;
                            }
                        }
                    }
                }
                let mut found = false;
                let entry_point_mgr = session.sync_odoo.entry_point_mgr.clone();
                let entry_point_mgr = entry_point_mgr.borrow();
                let from_path = session.sync_odoo.entry_point_mgr.borrow().transform_addon_path(&PathBuf::from(from_path));
                let from_path = PathBuf::from(from_path);
                for entry in entry_point_mgr.iter_for_import(for_entry) {
                    if ((entry.borrow().is_public() && (level.is_none() || level.unwrap() == 0)) || entry.borrow().is_valid_for(&from_path)) && entry.borrow().addon_to_odoo_path.is_none() {
                        let entry_point = entry.borrow().get_symbol();
                        if let Some(entry_point) = entry_point {
                            let mut next_symbol = entry_point.borrow().get_symbol(&(vec![branch.clone()], vec![]), u32::MAX);
                            if next_symbol.is_empty() && matches!(entry_point.borrow().typ(), SymType::ROOT | SymType::NAMESPACE | SymType::PACKAGE(_) | SymType::COMPILED | SymType::DISK_DIR) {
                                next_symbol = match _resolve_new_symbol(session, entry_point.clone(), &branch, asname.clone()) {
                                    Ok(v) => vec![v],
                                    Err(_) => vec![]
                                }
                            }
                            if next_symbol.is_empty() {
                                continue;
                            }
                            if level.is_none() || level.unwrap() == 0 {
                                if entry.borrow().is_public() {
                                    if let Some(cache) = session.sync_odoo.import_cache.as_mut() {
                                        cache.modules.insert(branch.clone(), Some(next_symbol[0].clone()));
                                    }
                                } else if matches!(entry.borrow().typ, EntryPointType::MAIN | EntryPointType::ADDON) {
                                    if let Some(cache) = session.sync_odoo.import_cache.as_mut() {
                                        cache.main_modules.insert(branch.clone(), Some(next_symbol[0].clone()));
                                    }
                                }
                            }
                            found = true;
                            sym = Some(next_symbol[0].clone());
                            last_symbol = Some(next_symbol[0].clone());
                            break;
                        }
                    }
                }
                if !found {
                    if for_entry.borrow().typ != EntryPointType::CUSTOM {
                        if let Some(cache) = session.sync_odoo.import_cache.as_mut() {
                            if for_entry.borrow().typ == EntryPointType::MAIN || for_entry.borrow().typ == EntryPointType::ADDON {
                                cache.main_modules.insert(branch.clone(), None);
                            } else {
                                cache.modules.insert(branch.clone(), None);
                            }
                        }
                    }
                    sym = None;
                    last_symbol = None;
                    break;
                }
            }
        }
    }
    return (sym, last_symbol)
}

fn _resolve_new_symbol(session: &mut SessionInfo, parent: Rc<RefCell<Symbol>>, name: &OYarn, asname: Option<String>) -> Result<Rc<RefCell<Symbol>>, String> {
    let sym_name: String = match asname {
        Some(asname_inner) => asname_inner.clone(),
        None => name.to_string()
    };
    if (*parent).borrow().typ() == SymType::COMPILED {
        return Ok((*parent).borrow_mut().add_new_compiled(session, &sym_name, &S!("")));
    }
    let paths = (*parent).borrow().paths().clone();
    for path in paths.iter() {
        let mut full_path = Path::new(path.as_str()).join(name.to_string());
        for stub in session.sync_odoo.stubs_dirs.iter() {
            if path.as_str().to_string() == *stub {
                full_path = full_path.join(name.to_string());
            }
        }
        if is_dir_cs(full_path.sanitize()) && (is_file_cs(full_path.join("__init__").with_extension("py").sanitize()) ||
        is_file_cs(full_path.join("__init__").with_extension("pyi").sanitize())) {
            //module directory
            let _rc_symbol = Symbol::create_from_path(session, &full_path, parent.clone(), false);
            if _rc_symbol.is_some() {
                let _arc_symbol = _rc_symbol.unwrap();
                SyncOdoo::build_now(session, &_arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if is_file_cs(full_path.with_extension("py").sanitize()) {
            let _arc_symbol = Symbol::create_from_path(session, &full_path.with_extension("py"), parent.clone(), false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                SyncOdoo::build_now(session, &_arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if is_file_cs(full_path.with_extension("pyi").sanitize()) {
            let _arc_symbol = Symbol::create_from_path(session, &full_path.with_extension("pyi"), parent.clone(), false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                SyncOdoo::build_now(session, &_arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if is_dir_cs(full_path.sanitize()) {
            //namespace directory
            let _rc_symbol = Symbol::create_from_path(session, &full_path, parent.clone(), false);
            if _rc_symbol.is_some() {
                let _arc_symbol = _rc_symbol.unwrap();
                SyncOdoo::build_now(session, &_arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if !matches!(parent.borrow().typ(), SymType::ROOT) {
            if cfg!(target_os = "windows") {
                for entry in glob((full_path.sanitize() + "*.pyd").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(_path) => {
                            return Ok((*parent).borrow_mut().add_new_compiled(session, &sym_name, &_path.to_str().unwrap().to_string()));
                        }
                        Err(_) => {},
                    }
                }
            } else if cfg!(target_os = "linux") {
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

pub fn get_all_valid_names(session: &mut SessionInfo, source_file_symbol: &Rc<RefCell<Symbol>>, from_stmt: Option<&Identifier>, base_name: String, level: Option<u32>) -> HashSet<OYarn> {
    //A: search base of different imports
    let source_root = source_file_symbol.borrow().get_root().as_ref().unwrap().upgrade().unwrap();
    let entry = source_root.borrow().get_entry().unwrap();
    let _source_file_symbol_lock = source_file_symbol.borrow_mut();
    let file_tree = _resolve_packages(
        &_source_file_symbol_lock,
        level,
        from_stmt);
    drop(_source_file_symbol_lock);
    let mut start_symbol = None;
    if level.is_some() {
        //if level is some, resolve_pacackages already built a full tree, so we can start from root
        start_symbol = Some(source_root.clone());
    }
    let source_path = source_file_symbol.borrow().paths()[0].clone();
    let (from_symbol, _fallback_sym) = _get_or_create_symbol(session,
        &entry,
        source_path.as_str(),
        start_symbol,
        &file_tree,
        None,
        level);
    let mut result = HashSet::new();
    if from_symbol.is_none() {
        return result;
    }
    let from_symbol = from_symbol.unwrap();

    let mut sym: Option<Rc<RefCell<Symbol>>> = Some(from_symbol.clone());
    let mut names = vec![base_name.split(".").map(|s| oyarn!("{}", s)).next().unwrap()];
    if base_name.ends_with(".") {
        names.push(Sy!(""));
    }
    for (index, branch) in names.iter().enumerate() {
        if index != names.len() -1 {
            let mut next_symbol = sym.as_ref().unwrap().borrow().get_symbol(&(vec![branch.clone()], vec![]), u32::MAX);
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
            if symbol.borrow().name().starts_with(filter.as_str()) {
                result.insert(symbol.borrow().name().clone());
            }
        }
    }

    return result;
}
