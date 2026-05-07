use glob::glob;
use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::name::Name;
use tracing::error;
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use ruff_text_size::TextRange;
use ruff_python_ast::{Alias, AtomicNodeIndex, Identifier};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::{ModuleKey, NamespaceKey, SourceFileKey, SymbolKey};
use crate::{constants::*, oyarn, Sy, S};
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::threads::SessionInfo;
use crate::utils::{is_dir_cs, is_file_cs, PathSanitizer};

use super::entry_point::{EntryPoint, EntryPointType};
use super::odoo::SyncOdoo;

pub struct ImportResult {
    pub name: OYarn, //the last imported element
    pub var_name: OYarn, // the effective symbol name (asname, or first part in a import A.B.C)
    pub found: bool,
    pub symbols: Vec<SymbolKey>,
    pub file_tree: Vec<OYarn>, //contains only the first part of a Tree
    pub range: TextRange,
}

//class used to cache import results in a build execution to speed up subsequent imports.
//It means of course than a modification during the build will not be taken into account, but it should be ok because reloaded after the build
#[derive(Debug)]
pub struct ImportCache {
    pub modules: HashMap<OYarn, Option<Vec<SymbolKey>>>,
    pub main_modules: HashMap<OYarn, Option<Vec<SymbolKey>>>,
}

fn resolve_import_stmt_hook(alias: &Alias, from_symbols: &Option<Vec<SymbolKey>>, session: &mut SessionInfo, source_file_symbol: SymbolKey, from_stmt: Option<&Identifier>, level: u32, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Option<ImportResult>{
    if !(session.sync_odoo.version_major >= 17 && alias.name.as_str() == "Form"){
        return None;
    }
    for &from_symbol in from_symbols.iter().flatten() {
        if session.sync_odoo.get_main_entry_tree(from_symbol).0 != vec!["odoo", "tests", "common"] {
            continue;
        }
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
        return results.pop()
    }
    None
}

/**
 * Helper to manually import a symbol. Do not forget to use level instead of '.' in the from_stmt parameter.
 */
pub fn manual_import(session: &mut SessionInfo, source_file_symbol: SymbolKey, from_stmt:Option<String>, name: &str, asname: Option<String>, level: u32, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Vec<ImportResult> {
    let name_aliases = vec![Alias {
        name: Identifier { id: Name::new(name), range: TextRange::default(), node_index: AtomicNodeIndex::default() },
        asname: match asname {
            Some(asname_inner) => Some(Identifier { id: Name::new(asname_inner), range: TextRange::default(), node_index: AtomicNodeIndex::default() }),
            None => None,
        },
        range: TextRange::default(),
        node_index: AtomicNodeIndex::default()
    }];
    let from_stmt = match from_stmt {
        Some(from_stmt_inner) => Some(Identifier { id: Name::new(from_stmt_inner), range: TextRange::default(), node_index: AtomicNodeIndex::default() }),
        None => None,
    };
    resolve_import_stmt(session, source_file_symbol, from_stmt.as_ref(), &name_aliases, level, diagnostics)
}

/// Resolve the base symbol from a from statement and level
pub fn resolve_from_stmt(
    session: &mut SessionInfo, source_file_symbol: SymbolKey, from_stmt: Option<&Identifier>, level: u32
) -> (Option<Vec<SymbolKey>>, Option<Vec<SymbolKey>>, Vec<OYarn>) {
    let symbol_table = &session.sync_odoo.symbol_table;
    let source_root: SymbolKey = symbol_table.get_root(source_file_symbol).into();
    let entry = symbol_table.get_entry(source_root);
    let Some(file_tree) = resolve_packages(
        symbol_table,
        source_file_symbol,
        level,
        from_stmt) else {
        return (None, Some(vec![source_root]), vec![]);
    };
    let source_path = symbol_table.paths(source_file_symbol)[0].clone();
    let mut start_symbol = None;
    if level != 0 {
        //if level is *not* 0 (relative import), resolve_packages already built a full tree, so we can start from root
        start_symbol = Some(vec![source_root]);
    }
    let (from_symbol, fallback_sym) = get_or_create_symbol(session,
        &entry,
        source_path.as_str(),
        start_symbol,
        &file_tree,
        None,
        level);
    let fallback_sym = Some(fallback_sym.unwrap_or(vec![source_root]));
    (from_symbol, fallback_sym, file_tree)
}

// Despite its name, `source_file_symbol` is not necessarily a SourceFileKey. It could be a namespace, for example.
pub fn resolve_import_stmt(session: &mut SessionInfo, source_file_symbol: SymbolKey, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: u32, diagnostics: &mut Option<&mut Vec<Diagnostic>>) -> Vec<ImportResult> {
    //A: search base of different imports
    let source_root: SymbolKey = session.st().get_root(source_file_symbol).into();
    let entry = session.st().get_entry(source_root);
    let (from_symbols, fallback_syms, file_tree) = resolve_from_stmt(session, source_file_symbol, from_stmt, level);
    let mut result = vec![];
    for alias in name_aliases {
        result.push(ImportResult{
            name: OYarn::from(alias.name.as_ref().to_string()),
            var_name: OYarn::from(alias.asname.as_ref().unwrap_or(&alias.name).to_string()),
            found: false,
            symbols: fallback_syms.as_ref().unwrap().clone(),
            file_tree: file_tree.clone(),
            range: alias.range.clone()
        })
    }
    if from_symbols.is_none()
    && (from_stmt.is_some() || level != 0)
    {
        return result;
    }

    let mut name_index: i32 = -1;
    for alias in name_aliases.iter() {
        let name = alias.name.as_str();
        name_index += 1;
        if let Some(hook_result) = resolve_import_stmt_hook(alias, &from_symbols, session, source_file_symbol, from_stmt, level,  diagnostics){
            result[name_index as usize] = hook_result;
            continue;
        }
        if name == "*" {
            result[name_index as usize].found = true;
            result[name_index as usize].symbols = from_symbols.as_ref().unwrap().clone();
            continue;
        }
        let name_split: Vec<OYarn> = name.split(".").map(|s| oyarn!("{}", s)).collect();
        let name_first_part: Vec<OYarn> = vec![name_split.first().unwrap().clone()];
        let name_middle_part: Vec<OYarn> = if name_split.len() > 2 {
            Vec::from_iter(name_split[1..name_split.len()-1].iter().cloned())
        } else {
            vec![]
        };
        let source_path = session.st().paths(source_file_symbol)[0].clone();
        let name_last_name: Vec<OYarn> = vec![name_split.last().unwrap().clone()];
        let (mut next_symbol, mut fallback_sym) = get_or_create_symbol(
            session,
            &entry,
            source_path.as_str(),
            from_symbols.clone(),
            &name_first_part,
            None,
        0);
        if next_symbol.is_none() && name_split.len() == 1 && from_symbols.is_some() {
            //check the last name is not a symbol in the file
            let name_symbol_vec = from_symbols.as_ref().unwrap().iter().flat_map(|&s| session.st().get_symbol(s, &(vec![], name_first_part.clone()), u32::MAX)).collect::<Vec<_>>();
            next_symbol = if name_symbol_vec.len() > 0 {Some(name_symbol_vec.clone())} else {None};
        }
        if next_symbol.is_none() {
            result[name_index as usize].symbols = fallback_sym.clone().unwrap_or_else(|| vec![source_root]);
            continue;
        }
        if alias.asname.is_none() {
            // If asname is not defined, we only have to return the first symbol found. However we have to search for the other names to import them too
            // In all "from X import A" case, it simply means search for A
            // But in "import A.B.C", it means search for A only, and import B.C
            // If user typed import A.B.C as D, we will search for A.B.C to link it to symbol D,
            result[name_index as usize].var_name = name.split(".").map(|s| oyarn!("{}", s)).next().unwrap();
            //result[name_index as usize].found = true; //even if found at this stage, we want to check everything anyway for diagnostics. But if found, we'll keep this symbol as imported
            result[name_index as usize].symbols = next_symbol.as_ref().unwrap().clone();
        }
        if !name_middle_part.is_empty() {
            (next_symbol, fallback_sym) = get_or_create_symbol(
                session,
                &entry,
                "",
                Some(next_symbol.as_ref().unwrap().clone()),
                &name_middle_part,
                None,
            0);
        }
        if next_symbol.is_none() {
            if alias.asname.is_some() {
                result[name_index as usize].symbols = fallback_sym.clone().unwrap_or_else(|| vec![source_root]);
            }
            continue;
        }
        if name_split.len() > 1 {
            // now we can search for the last symbol, or create it if it doesn't exist
            let (mut last_symbol, fallback_sym) = get_or_create_symbol(
                session,
                &entry,
                "",
                Some(next_symbol.as_ref().unwrap().clone()),
                &name_last_name,
                None,
            0);
            if last_symbol.is_none() { //If not a file/package, try to look up in symbols in current file (second parameter of get_symbol)
                //TODO what if multiple values?
                let name_symbol_vec = next_symbol.as_ref().unwrap().iter().flat_map(|&s| session.st().get_symbol(s, &(vec![], name_last_name.clone()), u32::MAX)).collect::<Vec<_>>();
                last_symbol = if name_symbol_vec.len() > 0 {Some(name_symbol_vec.clone())} else {None};
                if last_symbol.is_none() {
                    if alias.asname.is_some() {
                        result[name_index as usize].symbols = fallback_sym.clone().unwrap_or_else(|| vec![source_root]);
                    }
                    continue;
                }
            }
            // we found it ! store the result if not already done
            if result[name_index as usize].found == false {
                result[name_index as usize].found = true;
                if alias.asname.is_some() {
                    result[name_index as usize].symbols = last_symbol.as_ref().unwrap().clone();
                }
            }
        } else {
            //everything is ok, let's store the result if not already done
            result[name_index as usize].name = name.split(".").map(|s| oyarn!("{}", s)).next().unwrap();
            result[name_index as usize].found = true;
            result[name_index as usize].symbols = next_symbol.as_ref().unwrap().clone();
        }
    }

    return result;
}

pub fn create_module_from_name(session: &mut SessionInfo, odoo_addons: NamespaceKey, name: &str) -> Option<ModuleKey> {
    for path in session.st()[odoo_addons].paths() {
        let full_path = PathBuf::from(path).join(name);
        if !is_dir_cs(full_path.sanitize()) {
            continue;
        }
        let Some(module) = SymbolTable::create_module_from_path(session, &full_path, odoo_addons) else {
            continue;
        };
        SyncOdoo::build_now(session, module, BuildSteps::ARCH);
        return Some(module);
    }
    None
}

fn resolve_packages(symbol_table: &SymbolTable, from_file: SymbolKey, level: u32, from_stmt: Option<&Identifier>) -> Option<Vec<OYarn>> {
    let mut first_part_tree: Vec<OYarn> = vec![];
    if level > 0 {
        let mut lvl = level;
        let paths = symbol_table.paths(from_file);
        if lvl > Path::new(&paths[0]).components().count() as u32 {
            return None;
        }
        if matches!(from_file.typ(), SymType::PACKAGE(_)) {
            lvl -= 1;
        }
        if lvl == 0 {
            first_part_tree = symbol_table.get_tree(from_file).0.clone();
        } else {
            let tree = symbol_table.get_tree(from_file);
            if lvl > tree.0.len() as u32 {
                error!("Level is too high and going out of scope");
                return None;
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
    Some(first_part_tree)
}

fn get_or_create_symbol(
    session: &mut SessionInfo, for_entry: &Rc<RefCell<EntryPoint>>, from_path: &str, symbol: Option<Vec<SymbolKey>>, names: &Vec<OYarn>, asname: Option<String>, level: u32
) -> (Option<Vec<SymbolKey>>, Option<Vec<SymbolKey>>) {
    let mut syms = symbol.clone();
    let mut last_symbols = symbol.clone();
    for branch in names.iter() {
        if branch.is_empty() {
            continue;
        }
        if matches!(&syms, Some(s) if s.len() == 0) {
            syms = None;
        }
        match syms {
            Some(ref symbols) => {
                let mut next_symbol = vec![];
                for &s in symbols.iter() {
                    let mut current_batch_symbol = session.st().get_symbol(s, &(vec![branch.clone()], vec![]), u32::MAX);
                    if current_batch_symbol.is_empty() && matches!(s, SymbolKey::Root(_) | SymbolKey::Namespace(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) | SymbolKey::Compiled(_) | SymbolKey::DiskDir(_)) {
                        current_batch_symbol = match resolve_new_symbol(session, s, &branch, asname.clone()) {
                            Ok(v) => vec![v],
                            Err(_) => vec![]
                        }
                    }
                    next_symbol.extend(current_batch_symbol.clone());
                }
                if next_symbol.is_empty() {
                    syms = None;
                    break;
                }
                syms = Some(next_symbol.clone());
                last_symbols = Some(next_symbol.clone());
            },
            None => {
                // Can we have sym None and level != 0 ? maybe on get_all_valid_names? tbc
                if level == 0 {
                    if let Some(ref cache) = session.sync_odoo.import_cache {
                        let cache_module = if for_entry.borrow().typ == EntryPointType::MAIN || for_entry.borrow().typ == EntryPointType::ADDON {
                            cache.main_modules.get(branch)
                        } else {
                            cache.modules.get(branch)
                        };
                        if let Some(symbol) = cache_module {
                            if let Some(symbol) = symbol {
                                syms = Some(symbol.clone());
                                last_symbols = Some(symbol.clone());
                                continue;
                            } else {
                                //we know we won't find it
                                syms = None;
                                last_symbols = None;
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
                    if ((entry.borrow().is_public() && level == 0) || entry.borrow().is_valid_for(&from_path)) && entry.borrow().addon_to_odoo_path.is_none() {
                        let entry_point = entry.borrow().get_symbol(session.st());
                        if let Some(entry_point) = entry_point {
                            let mut next_symbols = session.st().get_symbol(entry_point, &(vec![branch.clone()], vec![]), u32::MAX);
                            if next_symbols.is_empty() && matches!(entry_point, SymbolKey::Root(_) | SymbolKey::Namespace(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) | SymbolKey::Compiled(_) | SymbolKey::DiskDir(_)) {
                                next_symbols = match resolve_new_symbol(session, entry_point, &branch, asname.clone()) {
                                    Ok(v) => vec![v],
                                    Err(_) => vec![]
                                }
                            }
                            if next_symbols.is_empty() {
                                continue;
                            }
                            if level == 0 {
                                if entry.borrow().is_public() {
                                    if let Some(cache) = session.sync_odoo.import_cache.as_mut() {
                                        cache.modules.insert(branch.clone(), Some(next_symbols.clone()));
                                    }
                                } else if matches!(entry.borrow().typ, EntryPointType::MAIN | EntryPointType::ADDON) {
                                    if let Some(cache) = session.sync_odoo.import_cache.as_mut() {
                                        cache.main_modules.insert(branch.clone(), Some(next_symbols.clone()));
                                    }
                                }
                            }
                            found = true;
                            syms = Some(next_symbols.clone());
                            last_symbols = Some(next_symbols.clone());
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
                    syms = None;
                    last_symbols = None;
                    break;
                }
            }
        }
    }
    return (syms, last_symbols)
}

/// Resolve a new symbol from disk, creating it if found, or just creating a COMPILED symbol if a parent is COMPILED
/// parent : parent symbol where to search, either ROOT, NAMESPACE, PACKAGE, COMPILED or DISK_DIR
fn resolve_new_symbol(session: &mut SessionInfo, parent: SymbolKey, imported_name: &OYarn, asname: Option<String>) -> Result<SymbolKey, String> {
    if imported_name == "" {
        return Err("Empty name".to_string());
    }
    let sym_name: String = match asname {
        Some(asname_inner) => asname_inner,
        None => imported_name.to_string()
    };
    // COMPILED: we can only create a COMPILED symbol
    if matches!(parent, SymbolKey::Compiled(_)) {
        return Ok(session.st_mut().add_new_compiled(parent, &sym_name, "").into());
    }
    // ROOT, NAMESPACE, PACKAGE or DISK_DIR: we can search on disk
    let paths = session.st().paths(parent);
    for path in paths.iter() {
        let mut full_path = Path::new(path.as_str()).join(imported_name.to_string());
        for stub in session.sync_odoo.stubs_dirs.iter() {
            if path.as_str().to_string() == *stub {
                full_path = full_path.join(imported_name.to_string());
            }
        }
        if is_dir_cs(full_path.sanitize()) && (is_file_cs(full_path.join("__init__").with_extension("py").sanitize().as_str()) ||
        is_file_cs(full_path.join("__init__").with_extension("pyi").sanitize().as_str())) {
            //module directory
            let _rc_symbol = SymbolTable::create_from_path(session, &full_path, parent, false);
            if _rc_symbol.is_some() {
                let _arc_symbol = _rc_symbol.unwrap();
                SyncOdoo::build_now(session, _arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if is_file_cs(full_path.with_extension("py").sanitize().as_str()) {
            let _arc_symbol = SymbolTable::create_from_path(session, &full_path.with_extension("py"), parent, false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                SyncOdoo::build_now(session, _arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if is_file_cs(full_path.with_extension("pyi").sanitize().as_str()) {
            let _arc_symbol = SymbolTable::create_from_path(session, &full_path.with_extension("pyi"), parent, false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                SyncOdoo::build_now(session, _arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if is_dir_cs(full_path.sanitize()) {
            //namespace directory
            let _rc_symbol = SymbolTable::create_from_path(session, &full_path, parent, false);
            if _rc_symbol.is_some() {
                let _arc_symbol = _rc_symbol.unwrap();
                SyncOdoo::build_now(session, _arc_symbol, BuildSteps::ARCH);
                return Ok(_arc_symbol);
            }
        } else if !matches!(parent, SymbolKey::Root(_)) {
            if cfg!(target_os = "windows") {
                for entry in glob((full_path.sanitize() + "*.pyd").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(_path) => {
                            return Ok(session.st_mut().add_new_compiled(parent, &sym_name, _path.to_str().unwrap()).into());
                        }
                        Err(_) => {},
                    }
                }
            } else if cfg!(target_os = "linux") {
                for entry in glob((full_path.sanitize() + "*.so").as_str()).expect("Failed to read glob pattern") {
                    match entry {
                        Ok(_path) => {
                            return Ok(session.st_mut().add_new_compiled(parent, &sym_name, _path.to_str().unwrap()).into());
                        }
                        Err(_) => {},
                    }
                }
            }
        }
    }
    return Err("Symbol not found".to_string())
}

/*
Used for autocompletion. Given a base_name, return all valid names that can be used to complete it.
is_from indicates if the completion item is the X in "from X import Y". Else it is Y from "import Y" or "from X import Y"
*/
pub fn get_all_valid_names(session: &mut SessionInfo, source_file_symbol: SourceFileKey, from_stmt: Option<String>, import: String, level: u32, is_from: bool) -> HashMap<OYarn, SymType> {
    let (identifier_from, to_complete) = match from_stmt {
        Some(from_stmt_inner) => {
            if is_from {
                let split = from_stmt_inner.split(".").collect::<Vec<&str>>();
                if split.len() > 1 {
                    (Some(Identifier::new(split[0..split.len()-1].join(".").as_str(), TextRange::default())), split.last().unwrap().to_string())
                } else {
                    (None, split.last().unwrap().to_string())
                }
            } else {
                (Some(Identifier::new(from_stmt_inner.clone(), TextRange::default())), import.clone())
            }
        },
        None => (None, import.split(".").last().unwrap().to_string()),
    };
    let entry = session.st().get_entry(source_file_symbol);
    let (mut from_symbol, _fallback_sym, file_tree) = resolve_from_stmt(session, source_file_symbol.into(), identifier_from.as_ref(), level);
    let source_path = session.st().path(source_file_symbol).to_string();
    let mut result = HashMap::new();
    let mut symbols_to_browse = vec![];
    if from_symbol.is_none() {
        if !file_tree.is_empty() { //symbol was not found
            return result;
        } else { //nothing was provided, so we have to add the root symbol of any valid entrypoint
            let entry_point_mgr = session.sync_odoo.entry_point_mgr.clone();
            let entry_point_mgr = entry_point_mgr.borrow();
            let from_path = session.sync_odoo.entry_point_mgr.borrow().transform_addon_path(&PathBuf::from(&source_path));
            let from_path = PathBuf::from(from_path);
            for entry in entry_point_mgr.iter_for_import(&entry) {
                if (entry.borrow().is_public() && (level == 0)) || entry.borrow().is_valid_for(&from_path) {
                    let entry_point = entry.borrow().get_symbol(session.st());
                    if let Some(entry_point) = entry_point {
                        symbols_to_browse.push(entry_point);
                    }
                }
            }
            if symbols_to_browse.is_empty() {
                return result;
            }
        }
    }
    if is_from {
        if let Some(fs) = from_symbol {
            symbols_to_browse.extend(fs);
        }
        for &symbol_to_browse in symbols_to_browse.iter() {
            let valid_names = valid_names_for_a_symbol(session.st(), symbol_to_browse, &to_complete, true);
            result.extend(valid_names);
        }
        return result;
    }

    let import_parts = import.split(".").collect::<Vec<&str>>();
    if import_parts.len() > 1 {
        let (next_symbol, _fallback_sym) = get_or_create_symbol(
            session,
            &entry,
            &source_path,
            from_symbol.clone(),
            &import_parts[0..import_parts.len()-1].iter().map(|s| oyarn!("{}", *s)).collect(),
            None,
            level,
        );
        if next_symbol.is_none() {
            return result;
        }
        from_symbol = next_symbol.clone();
    }
    if let Some(fs) = from_symbol {
        symbols_to_browse.clear();
        symbols_to_browse.extend(fs);
    }
    for &symbol_to_browse in symbols_to_browse.iter() {
        let valid_names = valid_names_for_a_symbol(session.st(), symbol_to_browse, &to_complete, false);
        result.extend(valid_names);
    }
    result
}

fn valid_names_for_a_symbol(symbol_table: &SymbolTable, symbol: SymbolKey, start_filter: &str, only_on_disk: bool) -> HashMap<OYarn, SymType> {
    let mut res = HashMap::new();
    match symbol {
        SymbolKey::File(_) => {
            if only_on_disk {
                return res;
            }
            res.extend(valid_name_from_symbol(symbol_table, symbol, start_filter));
        },
        SymbolKey::Namespace(_) | SymbolKey::DiskDir(_) => {
            for path in symbol_table.paths(symbol).iter() {
                res.extend(valid_name_from_disk(path, start_filter));
            }
        },
        SymbolKey::PythonPackage(_) | SymbolKey::Module(_) => {
            for path in symbol_table.paths(symbol).iter() {
                res.extend(valid_name_from_disk(path, start_filter));
            }
            if only_on_disk {
                return res;
            }
            res.extend(valid_name_from_symbol(symbol_table, symbol, start_filter));
        }
        SymbolKey::Class(_) | SymbolKey::Compiled(_) | SymbolKey::CsvFile(_) | SymbolKey::XmlFile(_) |
        SymbolKey::Function(_) | SymbolKey::Root(_) | SymbolKey::Variable(_) | SymbolKey::XmlRecord(_) |
        SymbolKey::XmlMenuItem(_) | SymbolKey::XmlTemplate(_) | SymbolKey::XmlAsset(_) | SymbolKey::XmlDelete(_) |
        SymbolKey::XmlField(_) => {
        }
    }
    res
}

fn valid_name_from_disk(path: &String, start_filter: &str) -> HashMap<OYarn, SymType> {
    let mut res = HashMap::new();
    if is_dir_cs(path.clone()) {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let Ok(file_type) = entry.file_type() else {
                        continue;
                    };
                    if file_type.is_dir() {
                        let dir_name = entry.file_name();
                        let dir_name_str = dir_name.to_string_lossy();
                        if dir_name_str.starts_with(start_filter) {
                            let mut typ = SymType::NAMESPACE;
                            if Path::new(&path).join(dir_name_str.to_string()).join("__init__.py").exists() {
                                typ = SymType::PACKAGE(PackageType::PYTHON_PACKAGE);
                            }
                            res.insert(Sy!(dir_name_str.to_string()), typ);
                        }
                    } else if file_type.is_file() {
                        let file_name = entry.file_name();
                        let file_name_str = file_name.to_string_lossy().to_string();
                        if (file_name_str.ends_with(".py") || file_name_str.ends_with(".pyi")) && file_name_str.starts_with(start_filter) {
                            let Some(stem) = Path::new(&file_name_str).file_stem() else {continue};
                            let Some(filename) = stem.to_str() else {continue};
                            if filename == "__init__" {continue;}
                            res.insert(Sy!(filename.to_string()), SymType::FILE);
                        }
                    }
                    //TODO support for symlinks?
                }
            }
        }
    }
    res
}

fn valid_name_from_symbol(symbol_table: &SymbolTable, symbol: SymbolKey, start_filter: &str) -> HashMap<OYarn, SymType> {
    let mut res = HashMap::new();
    for s in symbol_table.iter_symbols(symbol) {
        if s.0.starts_with(start_filter) {
            let mut typ = SymType::VARIABLE;
            let a_section = s.1.iter().last(); //let's take the last section, anyway we can display only one icon
            if let Some(a_section) = a_section {
                let last = a_section.1.last();
                if let Some(&last) = last {
                    typ = last.typ();
                }
            }
            res.insert(s.0.clone(), typ);
        }
    }
    res
}
