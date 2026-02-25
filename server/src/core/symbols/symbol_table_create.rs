//! Symbol creation methods 
 
use std::path::PathBuf;

use ruff_text_size::{TextRange, TextSize};

use crate::core::symbols::file_symbol::FileSymbol;
use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::{constants::OYarn, core::symbols::{
    compiled_symbol::CompiledSymbol, disk_dir_symbol::DiskDirSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, symbol_mgr::SymbolMgr, variable_symbol::VariableSymbol
}, threads::SessionInfo, utils::PathSanitizer};

use crate::core::symbols::symbol_table::{FunctionKey, PackageKey, SymbolKey, SymbolTable, VariableKey};


impl SymbolTable {
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
        self.add_to_parent_symbols(parent, function_key.into(), &oyarn!("{}", name), range.start().to_u32());
        function_key
    }

    fn add_to_parent_symbols(&mut self, parent: SymbolKey, content: SymbolKey, name: &OYarn, position: u32) {
         match parent {
            SymbolKey::File(f) => {
                let file = &mut self.files[f];
                let section = file.get_section_for(position).index;
                file.add_symbol(content, &name, section);
            },
            SymbolKey::Package(p) => {
                match &mut self.packages[p] {
                    PackageSymbol::Module(m) => {
                        let section = m.get_section_for(position).index;
                        m.add_symbol(content, &name, section);
                    },
                    PackageSymbol::PythonPackage(p) => {
                        let section = p.get_section_for(position).index;
                        p.add_symbol(content, &name, section);
                    },
                }
            },
            SymbolKey::Class(c) => {
                let class = &mut self.classes[c];
                let section = class.get_section_for(position).index;
                class.add_symbol(content, &name, section);
            },
            SymbolKey::Function(f) => {
                let function = &mut self.functions[f];
                let section = function.get_section_for(position).index;
                function.add_symbol(content, &name, section);
            }
            _ => { panic!("Impossible to add a variable to a {}", self.get_symbol(parent).unwrap().typ()); }
        }
    }

}