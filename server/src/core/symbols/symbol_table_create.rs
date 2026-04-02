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

use crate::core::symbols::symbol_table::{ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, ModuleKey, NamespaceKey, PythonPackageKey, RootKey, SymbolKey, SymbolTable, VariableKey, XmlFileKey, get_main_entry_tree, get_sym, invalidate};
use tracing::info;


impl SymbolTable {
    pub fn new_root(&mut self) -> RootKey {
        let root_symbol = RootSymbol::new();
        self.roots.insert(root_symbol)
    }
    // @arena: parent is a verified existing key
    // Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, parent: SymbolKey, name: &str, path: &str) -> FileKey {
        let is_external = self.parent_is_external(parent);
        let file_symbol = FileSymbol::new(name, path, parent, is_external);
        let file_key = self.files.insert(file_symbol);
        self.register_in_parent(parent, file_key.into(), name, path);
        file_key
    }

    // @arena: parent is a verified existing key - Consider adding a validate_key method
    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, parent: SymbolKey, name: &str, path: &str, i_ext: &'static str) -> PythonPackageKey {
        let is_external = self.parent_is_external(parent);
        let package_symbol = PythonPackageSymbol::new(name, path, parent, is_external, i_ext);
        let package_key = self.python_packages.insert(package_symbol);
        self.register_in_parent(parent, package_key.into(), name, path);
        package_key
    }

    pub fn add_new_namespace(&mut self, parent: SymbolKey, name: &str, path: &str) -> NamespaceKey {
        let is_external = self.parent_is_external(parent);
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.register_in_parent(parent, namespace_key.into(), name, path);
        namespace_key
    }

    pub fn add_new_disk_dir(&mut self, parent: SymbolKey, name: &str, path: &str) -> DiskDirKey {
        let is_external = self.parent_is_external(parent);
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.register_in_parent(parent, disk_dir_key.into(), name, path);
        disk_dir_key
    }

    pub fn add_new_compiled(&mut self, parent: SymbolKey, name: &str, path: &str) -> CompiledKey {
        let is_external = self.parent_is_external(parent);
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key = self.compiled.insert(compiled_symbol);
        match parent {
            SymbolKey::Compiled(c) => {
                self.compiled[c].module_symbols.insert(oyarn!("{}", name), compiled_key.into());
            },
            _ => {
                self.register_in_parent(parent, compiled_key.into(), name, path);
            }
        }
        compiled_key
    }

    // @arena: not a method! (takes SessionInfo as arg)
    pub fn add_new_module_package(session: &mut SessionInfo, parent: SymbolKey, name: &str, path: &PathBuf) -> Option<ModuleKey> {
        let is_external = session.sync_odoo.symbol_table.parent_is_external(parent);
        let module = ModuleSymbol::new(session, name, path, parent, is_external)?;
        let symbol_table = &mut session.sync_odoo.symbol_table;
        let module_key = symbol_table.modules.insert(module);
        symbol_table.register_in_parent(parent, module_key.into(), name, &path.sanitize());
        Some(module_key)
    }

    // ====== Helpers for symbol creation ======

    // @arena: this would be simpler if is_external returned true for root
    fn parent_is_external(&self, parent: SymbolKey) -> bool {
        match parent {
            SymbolKey::Root(_) => true,
            _ => self.get_symbol_view(parent).expect("valid key").is_external(),
        }
    }

    fn register_in_parent(&mut self, parent: SymbolKey, child: SymbolKey, name: &str, path: &str) {
        match parent {
            SymbolKey::Namespace(n) => {
                self.add_file_to_namespace(n, child, name, path);
            },
            SymbolKey::PythonPackage(p) => {
                self.python_packages[p].module_symbols.insert(oyarn!("{}", name), child);
            },
            SymbolKey::Module(m) => {
                self.modules[m].module_symbols.insert(oyarn!("{}", name), child);
            },
            SymbolKey::Root(r) => {
                self.roots[r].module_symbols.insert(oyarn!("{}", name), child);
            },
            SymbolKey::DiskDir(d) => {
                self.disk_dirs[d].module_symbols.insert(oyarn!("{}", name), child);
            },
            _ => {
                panic!("Impossible to add a {} to a {}",
                    self.get_symbol_view(child).unwrap().typ(),
                    self.get_symbol_view(parent).unwrap().typ()
                );
            }
        }
    }
    
    fn add_file_to_namespace(&mut self, parent: NamespaceKey, file: SymbolKey, name: &str, path: &str) {
        let ns = &mut self.namespaces[parent];
        let best = ns.directories.iter()
            .enumerate()
            .filter(|(_, dir)| PathBuf::from(path).starts_with(&dir.path))
            .max_by_key(|(_, dir)| dir.path.len())
            .unwrap_or_else(|| panic!("Not valid path found to add the file ({}) to namespace {} with directories {:?}", path, ns.name, ns.directories))
            .0;
        ns.directories[best].module_symbols.insert(oyarn!("{}", name), file);
    }

    // @arena: consider taking &str for name
    pub fn add_new_variable(&mut self, parent: impl Into<SymbolKey>, name: OYarn, range: &TextRange) -> VariableKey {
        let parent = parent.into();
        let is_external = self.get_symbol_view(parent).expect("valid key").is_external();
        let variable_symbol = VariableSymbol::new(name.clone(), parent, range.clone(), is_external);
        let variable_key = self.variables.insert(variable_symbol);
        self.add_to_parent_symbols(parent, variable_key.into(), &name, range.start().to_u32());
        variable_key
    }
    pub fn add_new_function(&mut self, parent: SymbolKey, name: &str, range: &TextRange, body_start: &TextSize) -> FunctionKey {
        let is_external = self.get_symbol_view(parent).expect("valid key").is_external();
        let function_symbol = FunctionSymbol::new(name, parent, range.clone(), body_start.clone(), is_external);
        let function_key = self.functions.insert(function_symbol);
        self.add_to_parent_symbols(parent, function_key.into(), name, range.start().to_u32());
        function_key
    }

    pub fn add_new_class(&mut self, parent: SymbolKey, name: &String, range: &TextRange, body_start: &TextSize) -> ClassKey {
        let is_external = self.get_symbol_view(parent).expect("valid key").is_external();
        let class_symbol = ClassSymbol::new(name, parent, range.clone(), body_start.clone(), is_external);
        let class_key = self.classes.insert(class_symbol);
        self.add_to_parent_symbols(parent, class_key.into(), name, range.start().to_u32());
        class_key.into()
    }

    fn add_to_parent_symbols(&mut self, parent: SymbolKey, content: SymbolKey, name: &str, position: u32) {
         match parent {
            SymbolKey::File(f) => {
                let file = &mut self.files[f];
                let section = file.get_section_for(position).index;
                file.add_symbol(content, name, section);
            },
            SymbolKey::Module(m) => {
                let module = &mut self.modules[m];
                let section = module.get_section_for(position).index;
                module.add_symbol(content, name, section);
            },
            SymbolKey::PythonPackage(p) => {
                let package = &mut self.python_packages[p];
                let section = package.get_section_for(position).index;
                package.add_symbol(content, name, section);
            },
            SymbolKey::Class(c) => {
                let class = &mut self.classes[c];
                let section = class.get_section_for(position).index;
                class.add_symbol(content, name, section);
            },
            SymbolKey::Function(f) => {
                let function = &mut self.functions[f];
                let section = function.get_section_for(position).index;
                function.add_symbol(content, name, section);
            }
            _ => {
                panic!("Impossible to add a {} to a {}",
                    self.get_symbol_view(content).unwrap().typ(),
                    self.get_symbol_view(parent).unwrap().typ()
                );
            }
        }
    }


    pub fn add_new_xml_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> XmlFileKey {
        let parent_symbol = self.modules.get(parent).expect("valid key");
        let mut xml_file_symbol = XmlFileSymbol::new(name, path, parent.into(), parent_symbol.is_external);
        xml_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let xml_file_key = self.xml_files.insert(xml_file_symbol);
        self.register_data_file(parent, path, xml_file_key.into());
        xml_file_key
    }

    /// parent is a module package
    pub fn add_new_csv_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> CsvFileKey {
        let parent_symbol = self.modules.get(parent).expect("valid key");
        let mut csv_file_symbol = CsvFileSymbol::new(name, path, parent.into(), parent_symbol.is_external);
        csv_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let csv_file_key = self.csv_files.insert(csv_file_symbol);
        self.register_data_file(parent, path, csv_file_key.into());
        csv_file_key
    }

    /// parent is a module package
    fn register_data_file(&mut self, parent: ModuleKey, path: &str, data_file: SymbolKey) {
        let entry = self.get_entry(parent.into()).unwrap();
        entry.borrow_mut().data_symbols.insert(path.to_string(), data_file.into());

        self.modules[parent].data_symbols.insert(path.to_string(), data_file);
    }

    // @arena: this is not done in the original code.
    // unload_path does it, apparently
    fn unregister_data_file(&mut self, parent: ModuleKey, path: &str) {
        let entry = self.get_entry(parent.into()).unwrap();
        entry.borrow_mut().data_symbols.remove(path);

        self.modules[parent].data_symbols.remove(path);
    }

    // @arena: removes a symbol from its parent (not yet from the symbol table)
    // original code in unload + remove symbol: unwraps Option(parent) and the weak.upgrade.
    pub fn remove_symbol(&mut self, child: SymbolKey) {
        let child_symbol = self.get_symbol_view(child).expect("valid key");
        let child_name = child_symbol.name().clone();
        let parent = child_symbol.parent().expect("symbol should have a parent");
        if child_symbol.is_file_content() {
            match parent {
                SymbolKey::Class(c) => { self.classes[c].symbols.remove(&child_name); },
                SymbolKey::File(f) => { self.files[f].symbols.remove(&child_name); },
                SymbolKey::Function(f) => { self.functions[f].symbols.remove(&child_name); },
                SymbolKey::Module(m) => { self.modules[m].symbols.remove(&child_name); },
                SymbolKey::PythonPackage(p) => { self.python_packages[p].symbols.remove(&child_name); },
                SymbolKey::DiskDir(_) => { panic!("A disk directory can not contain python code") },
                SymbolKey::Compiled(_) => { panic!("A compiled symbol can not contain python code") },
                SymbolKey::Namespace(_) => { panic!("A namespace can not contain python code") },
                SymbolKey::Root(_) => { panic!("Root can not contain python code") },
                SymbolKey::Variable(_) => { panic!("A variable can not contain python code") }
                SymbolKey::XmlFile(_) => { panic!("An XML file symbol can not contain python code") }
                SymbolKey::CsvFile(_) => { panic!("A CSV file symbol can not contain python code") }
            };
        } else {
            match parent {
                SymbolKey::Class(_) => { panic!("A class can not contain a file structure") },
                SymbolKey::File(_) => { panic!("A file can not contain a file structure"); },
                SymbolKey::Function(_) => { panic!("A function can not contain a file structure") },
                SymbolKey::DiskDir(d) => { self.disk_dirs[d].module_symbols.remove(&child_name); },
                SymbolKey::Module(m) => match child {
                    SymbolKey::XmlFile(x) => {
                        self.unregister_data_file(m, &self.xml_files[x].path.clone());
                    },
                    SymbolKey::CsvFile(c) => {
                        self.unregister_data_file(m, &self.csv_files[c].path.clone());
                    },
                    _ => {
                        self.modules[m].module_symbols.remove(&child_name);
                    },
                },
                SymbolKey::PythonPackage(p) => {
                    self.python_packages[p].module_symbols.remove(&child_name);
                },
                SymbolKey::Compiled(c) => { self.compiled[c].module_symbols.remove(&child_name); },
                SymbolKey::Namespace(n) => {
                    for directory in self.namespaces[n].directories.iter_mut() {
                        directory.module_symbols.remove(&child_name);
                    }
                },
                SymbolKey::Root(r) => { self.roots[r].module_symbols.remove(&child_name); },
                SymbolKey::Variable(_) => { panic!("A variable can not contain a file structure"); }
                SymbolKey::XmlFile(_) => { panic!("An XML file symbol can not contain a file structure") }
                SymbolKey::CsvFile(_) => { panic!("A CSV file symbol can not contain a file structure") }
            };
        }
        // self.set_parent(child, None);
    }

}

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

//unload a symbol and subsymbols.
// @arena:  removes the entry from the symbol table
// remove_symbol only removes the symbol from its parent, and some extra clean up in case
// of data files. Consider moving the clean up to here. Or to split the remove
// from parent + cleanup in separate functions per key type, that would mirror the add_* methods.
pub fn unload(session: &mut SessionInfo, symbol: SymbolKey) {
    macro_rules! st { () => { session.sync_odoo.symbol_table } }
    /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
    let mut vec_to_unload = VecDeque::from([symbol]);
    while !vec_to_unload.is_empty() {
        let ref_to_unload = *vec_to_unload.front().unwrap();
        let sym_ref = get_sym!(st!(), ref_to_unload);
        // Unload children first
        let mut found_one = false;
        for sym in sym_ref.all_symbols() {
            found_one = true;
            vec_to_unload.push_front(sym);
        }
        if found_one {
            continue;
        }
        vec_to_unload.pop_front();
        if DEBUG_MEMORY && (sym_ref.typ() == SymType::FILE || matches!(sym_ref.typ(), SymType::PACKAGE(_))) {
            info!("Unloading symbol {:?} at {:?}", sym_ref.name(), sym_ref.paths());
        }
        let module = st!().find_module(ref_to_unload);
        //unload symbol
        let parent = *sym_ref.parent().as_ref().unwrap();
        st!().remove_symbol(ref_to_unload);
        if matches!(ref_to_unload, SymbolKey::File(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) | SymbolKey::XmlFile(_) | SymbolKey::CsvFile(_)) {
            invalidate(session, ref_to_unload, &BuildSteps::ARCH);
        }
        //check if we should not reimport automatically
        match ref_to_unload {
            SymbolKey::PythonPackage(p) => {
                let package = &st!().python_packages[p];
                if package.self_import {
                    session.sync_odoo.must_reload_paths.push((parent.into(), package.path.clone()));
                }
            }
            SymbolKey::File(f) => {
                let file = &st!().files[f];
                if file.self_import {
                    session.sync_odoo.must_reload_paths.push((parent.into(), file.path.clone()));
                }
            }
            _ => {}
        }
        match ref_to_unload {
            SymbolKey::Module(p) => {
                let m = &st!().modules[p];
                // @arena: because of this, values in sync_odoo.modules can be trusted (make it not a Weak then?)
                session.sync_odoo.modules.remove(m.dir_name.as_str());
            }
            SymbolKey::Class(c) => {
                let class = &st!().classes[c];
                if let Some(model_data) = class._model.as_ref() {
                    let model = session.sync_odoo.models.get(&model_data.name).cloned();
                    if let Some(model) = model {
                        model.borrow_mut().remove_symbol(session, c,  module);
                    }
                }
            }
            _ => {}
        }
        st!().remove(ref_to_unload);
    }
}
