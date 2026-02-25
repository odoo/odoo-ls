//! Symbol creation methods 
 
use std::path::PathBuf;

use ruff_text_size::{TextRange, TextSize};

use crate::constants::tree;
use crate::core::file_mgr::FileMgr;
use crate::core::symbols::class_symbol::ClassSymbol;
use crate::core::symbols::csv_file_symbol::CsvFileSymbol;
use crate::core::symbols::file_symbol::FileSymbol;
use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::root_symbol::RootSymbol;
use crate::core::symbols::xml_file_symbol::XmlFileSymbol;
use crate::{constants::OYarn, core::symbols::{
    compiled_symbol::CompiledSymbol, disk_dir_symbol::DiskDirSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, symbol_mgr::SymbolMgr, variable_symbol::VariableSymbol
}, threads::SessionInfo, utils::PathSanitizer};

use crate::core::symbols::symbol_table::{ClassKey, CsvFileKey, FunctionKey, PackageKey, RootKey, SymbolKey, SymbolTable, VariableKey, XmlFileKey, get_main_entry_tree};


impl SymbolTable {
    pub fn new_root(&mut self) -> RootKey {
        let root_symbol = RootSymbol::new();
        self.roots.insert(root_symbol)
    }
    // @arena: parent is a verified existing key
    // Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let file_symbol = FileSymbol::new(name, path, parent, is_external);
        let file_key = self.files.insert(file_symbol);
        self.register_in_parent(parent, file_key.into(), name, path);
        file_key.into()
    }

    // @arena: parent is a verified existing key - Consider adding a validate_key method
    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, parent: SymbolKey, name: &str, path: &str) -> PackageKey {
        let is_external = self.parent_is_external(parent);
        let package_symbol = PackageSymbol::new_python_package(name, path, parent, is_external);
        let package_key = self.packages.insert(package_symbol);
        self.register_in_parent(parent, package_key.into(), name, path);
        package_key
    }

    pub fn add_new_namespace(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.register_in_parent(parent, namespace_key.into(), name, path);
        namespace_key.into()
    }

    pub fn add_new_disk_dir(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.register_in_parent(parent, disk_dir_key.into(), name, path);
        disk_dir_key.into()
    }

    pub fn add_new_compiled(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key: SymbolKey = self.compiled.insert(compiled_symbol).into();
        match parent {
            SymbolKey::Namespace(n) => {
                self.namespaces.get_mut(n).unwrap().add_file(compiled_key, name, path);
            },
            SymbolKey::Package(p) => {
                self.packages.get_mut(p).unwrap().add_file(compiled_key, name);
            },
            SymbolKey::Root(r) => {
                self.roots.get_mut(r).unwrap().add_file(compiled_key, name);
            },
            SymbolKey::Compiled(c) => {
                self.compiled.get_mut(c).unwrap().add_compiled(compiled_key, name);
            }
            SymbolKey::DiskDir(d) => {
                self.disk_dirs.get_mut(d).unwrap().add_file(compiled_key, name);
            },
            _ => {
                panic!("Impossible to add a {} to a {}", 
                    self.get_symbol(compiled_key).unwrap().typ(), 
                    self.get_symbol(parent).unwrap().typ()
                );
            }
        }
        compiled_key
    }

    // @arena: not a method! (takes SessionInfo as arg)
    pub fn add_new_module_package(session: &mut SessionInfo, parent: SymbolKey, name: &str, path: &PathBuf) -> Option<SymbolKey> {
        let is_external = session.sync_odoo.symbol_table.parent_is_external(parent);
        let module = PackageSymbol::new_module_package(session, name, path, parent, is_external)?;
        let symbol_table = &mut session.sync_odoo.symbol_table;
        let module_key = symbol_table.packages.insert(module);
        symbol_table.register_in_parent(parent, module_key.into(), name, &path.sanitize());
        Some(module_key.into())
    }

    // ====== Helpers for symbol creation ======

    // @arena: this would be simpler if is_external returned true for root
    fn parent_is_external(&self, parent: SymbolKey) -> bool {
        match parent {
            SymbolKey::Root(_) => true,
            _ => self.get_symbol(parent).expect("valid key").is_external(),
        }
    }

    fn register_in_parent(&mut self, parent: SymbolKey, child: SymbolKey, name: &str, path: &str) {             
        match parent {
            SymbolKey::Namespace(n) => {
                self.namespaces.get_mut(n).unwrap().add_file(child, name, path);
            },
            SymbolKey::Package(p) => {
                self.packages.get_mut(p).unwrap().add_file(child, name);
            },
            SymbolKey::Root(r) => {
                self.roots.get_mut(r).unwrap().add_file(child, name);
            },
            SymbolKey::DiskDir(d) => {
                self.disk_dirs.get_mut(d).unwrap().add_file(child, name);
            },
            _ => {
                panic!("Impossible to add a {} to a {}", 
                    self.get_symbol(child).unwrap().typ(), 
                    self.get_symbol(parent).unwrap().typ()
                );
            }
        }
    }

    // @arena: consider taking &str for name
    pub fn add_new_variable(&mut self, parent: SymbolKey, name: OYarn, range: &TextRange) -> VariableKey {
        let is_external = self.get_symbol(parent).expect("valid key").is_external();
        let variable_symbol = VariableSymbol::new(name.clone(), parent, range.clone(), is_external);
        let variable_key = self.variables.insert(variable_symbol);
        self.add_to_parent_symbols(parent, variable_key.into(), &name, range.start().to_u32());
        variable_key
    } 
    pub fn add_new_function(&mut self, parent: SymbolKey, name: &str, range: &TextRange, body_start: &TextSize) -> FunctionKey {
        let is_external = self.get_symbol(parent).expect("valid key").is_external();
        let function_symbol = FunctionSymbol::new(name, parent, range.clone(), body_start.clone(), is_external);
        let function_key = self.functions.insert(function_symbol);
        self.add_to_parent_symbols(parent, function_key.into(), name, range.start().to_u32());
        function_key
    }

    pub fn add_new_class(&mut self, parent: SymbolKey, name: &String, range: &TextRange, body_start: &TextSize) -> ClassKey {
        let is_external = self.get_symbol(parent).expect("valid key").is_external();
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
            SymbolKey::Package(p) => {
                match &mut self.packages[p] {
                    PackageSymbol::Module(m) => {
                        let section = m.get_section_for(position).index;
                        m.add_symbol(content, name, section);
                    },
                    PackageSymbol::PythonPackage(p) => {
                        let section = p.get_section_for(position).index;
                        p.add_symbol(content, name, section);
                    },
                }
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
                    self.get_symbol(content).unwrap().typ(), 
                    self.get_symbol(parent).unwrap().typ()
                );
            }
        }
    }


    pub fn add_new_xml_file(&mut self, parent: PackageKey, name: &str, path: &str) -> XmlFileKey {
        let parent_symbol = self.packages.get(parent).expect("valid key");
        let mut xml_file_symbol = XmlFileSymbol::new(name, path, parent.into(), parent_symbol.is_external());
        xml_file_symbol.set_in_workspace(parent_symbol.in_workspace());
        let xml_file_key = self.xml_files.insert(xml_file_symbol);
        self.register_data_file(parent, path, xml_file_key.into());
        xml_file_key
    }

    pub fn add_new_csv_file(&mut self, parent: PackageKey, name: &str, path: &str) -> CsvFileKey {
        let parent_symbol = self.packages.get(parent).expect("valid key");
        let mut csv_file_symbol = CsvFileSymbol::new(name, path, parent.into(), parent_symbol.is_external());
        csv_file_symbol.set_in_workspace(parent_symbol.in_workspace());
        let csv_file_key = self.csv_files.insert(csv_file_symbol);
        self.register_data_file(parent, path, csv_file_key.into());
        csv_file_key
    }

    fn register_data_file(&mut self, parent: PackageKey, path: &str, data_file: SymbolKey) {
        let entry = self.get_entry(parent.into()).unwrap();
        entry.borrow_mut().data_symbols.insert(path.to_string(), data_file);

        let package = &mut self.packages[parent];
        package.as_module_package_mut().data_symbols.insert(path.to_string(), data_file);
    }

}

// @arena: associated function in SymbolTable?
///Given a path, create the appropriated symbol and attach it to the given parent
pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: SymbolKey, require_module: bool) -> Option<SymbolKey> {
    let symbol_table = &mut session.sync_odoo.symbol_table;
    let name: String = if path.is_dir() {
        path.components().last().unwrap().as_os_str().to_str().unwrap().to_string()
    } else {
        path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string()
    };
    let path_str = path.sanitize();
    if path_str.ends_with(".py") || path_str.ends_with(".pyi") || FileMgr::is_untitled(&path_str) {
        return Some(symbol_table.add_new_file(parent, &name, &path_str));
    }
    let main_entry_tree = get_main_entry_tree(session, parent);
    if main_entry_tree == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
        let module = SymbolTable::add_new_module_package(session, parent, &name, path);
        let symbol_table = &mut session.sync_odoo.symbol_table;
        if let Some(module) = module {
            let module_symbol = symbol_table.get_symbol(module).unwrap();
            let dir_name = module_symbol.as_module_package().dir_name.clone();
            session.sync_odoo.modules.insert(dir_name, module);
            return Some(module);
        } else if require_module {
            return None;
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let package_key = symbol_table.add_new_python_package(parent, &name, &path_str);
                if !path.join("__init__.py").exists() {
                    symbol_table.packages.get_mut(package_key).unwrap().set_i_ext("i");
                }
                return Some(package_key.into());
            } else {
                return None;
            }
        }
    } else if require_module {
        return None;
    } else {
        let symbol_table = &mut session.sync_odoo.symbol_table;
        if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
            if main_entry_tree == tree(vec!["odoo"], vec![]) && path_str.ends_with("addons") {
                //Force namespace for odoo/addons
                let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
                return Some(namespace_key);
            } else {
                // let ref_sym = parent.borrow_mut().add_new_python_package(session, &name, &path_str);
                let package_key = symbol_table.add_new_python_package(parent, &name, &path_str);
                if !path.join("__init__.py").exists() {
                    symbol_table.packages.get_mut(package_key).unwrap().set_i_ext("i");
                }
                return Some(package_key.into());
            }
        } else if path.is_dir() {
            let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
            return Some(namespace_key);
        }
    }
    None
}