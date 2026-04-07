//! Symbol creation methods

use std::collections::VecDeque;
use std::path::PathBuf;

use ruff_text_size::{TextRange, TextSize};

use crate::constants::{BuildSteps, DEBUG_MEMORY, SymType, tree};
use crate::core::file_mgr::FileMgr;
use crate::core::symbols::class_symbol::ClassSymbol;
use crate::core::symbols::csv_file_symbol::CsvFileSymbol;
use crate::core::symbols::dependency_mgr::Dependencies;
use crate::core::symbols::file_symbol::FileSymbol;
use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::package_symbol::PythonPackageSymbol;
use crate::core::symbols::root_symbol::RootSymbol;
use crate::core::symbols::xml_file_symbol::XmlFileSymbol;
use crate::oyarn;
use crate::{constants::OYarn, core::symbols::{
    compiled_symbol::CompiledSymbol, disk_dir_symbol::DiskDirSymbol, namespace_symbol::NamespaceSymbol, symbol_mgr::SymbolMgr, variable_symbol::VariableSymbol
}, threads::SessionInfo, utils::PathSanitizer};

use crate::core::symbols::symbol_table::{ModuleKey, SymbolKey, SymbolTable};
use crate::core::symbols::symbol_table_ops::get_main_entry_tree;
use tracing::info;



pub fn create_module_from_path(session: &mut SessionInfo, path: &PathBuf, parent: SymbolKey) -> Option<ModuleKey> {
    let main_entry_tree = get_main_entry_tree(session, parent);
    if !(main_entry_tree == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists()) {
        return None;
    }
    let name = path.components().last().unwrap().as_os_str().to_str().unwrap();
    let module = SymbolTable::add_new_module_package(session, parent, &name, path)?;
    let dir_name = session.sync_odoo.symbol_table.modules[module].dir_name.clone();
    session.sync_odoo.modules.insert(dir_name, module.into());
    return Some(module);
}

// @arena: associated function in SymbolTable?
///Given a path, create the appropriated symbol and attach it to the given parent
pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: SymbolKey, require_module: bool) -> Option<SymbolKey> {
    if require_module {
        return create_module_from_path(session, path, parent).map(SymbolKey::from)
    }
    let symbol_table = &mut session.sync_odoo.symbol_table;
    let name: String = if path.is_dir() {
        path.components().last().unwrap().as_os_str().to_str().unwrap().to_string()
    } else {
        path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string()
    };
    let path_str = path.sanitize();
    if path_str.ends_with(".py") || path_str.ends_with(".pyi") || FileMgr::is_untitled(&path_str) {
        return Some(symbol_table.add_new_file(parent, &name, &path_str).into());
    }
    let main_entry_tree = get_main_entry_tree(session, parent);
    if main_entry_tree == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
        let module = SymbolTable::add_new_module_package(session, parent, &name, path);
        let symbol_table = &mut session.sync_odoo.symbol_table;
        if let Some(module) = module {
            let dir_name = symbol_table.modules[module].dir_name.clone();
            session.sync_odoo.modules.insert(dir_name, module.into());
            return Some(module.into());
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let i_ext = if path.join("__init__.py").exists() { "" } else { "i" };
                let package_key = symbol_table.add_new_python_package(parent, &name, &path_str, i_ext);
                return Some(package_key.into());
            } else {
                return None;
            }
        }
    } else {
        let symbol_table = &mut session.sync_odoo.symbol_table;
        if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
            if main_entry_tree == tree(vec!["odoo"], vec![]) && path_str.ends_with("addons") {
                //Force namespace for odoo/addons
                let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
                return Some(namespace_key.into());
            } else {
                let i_ext = if path.join("__init__.py").exists() { "" } else { "i" };
                let package_key = symbol_table.add_new_python_package(parent, &name, &path_str, i_ext);
                return Some(package_key.into());
            }
        } else if path.is_dir() {
            let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
            return Some(namespace_key.into());
        }
    }
    None
}


