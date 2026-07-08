//! Symbol creation and destruction.

/// All slotmap insertions/removals and parent/child relationship mutations are
/// centralized here.
///
/// The `symbols` and `module_symbols` fields on variant structs
/// are `pub(super)`, so only code within the storage module can mutate them.
/// Combined with private slotmaps, this guarantees that `parent`, `symbols`, and
/// `module_symbols` always hold valid keys — they can be trusted without validity
/// checks, unlike keys stored elsewhere (as Weak).

use crate::{
    constants::{OYarn, PackageType, SymType},
    core::{
        entry_point::{EntryPoint, EntryPointCleanupToken}, odoo::SyncOdoo, symbols::{
            ClassSymbol, CompiledSymbol, CsvFileSymbol, Dependencies, DiskDirSymbol, FileSymbol, FunctionSymbol, JsFileSymbol, ModuleSymbol, NamespaceSymbol, PythonPackageSymbol, RootSymbol, SymbolTable, VariableSymbol, XmlFileSymbol, storage::xml::{xml_asset_symbol::XmlAssetSymbol, xml_delete_symbol::XmlDeleteSymbol, xml_field_symbol::XmlFieldSymbol, xml_menuitem_symbol::XmlMenuItemSymbol, xml_record_symbol::XmlRecordSymbol, xml_template_symbol::XmlTemplateSymbol}, symbol_keys::{
                ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, JsFileKey, JsFileParent, KeyValidator, ModuleKey, NamespaceKey, PythonPackageKey, RootKey, SourceFileKey, SymbolKey, VariableKey, Wk, XmlAssetKey, XmlDataKey, XmlDeleteKey, XmlFieldKey, XmlFileKey, XmlMenuItemKey, XmlRecordKey, XmlTemplateKey
            }, symbol_mgr::SymbolMgr
        }
    },
    oyarn,
    threads::SessionInfo,
};
use ruff_text_size::{TextRange, TextSize};
use std::{cell::RefCell, ops::Range, path::PathBuf, rc::Rc};

impl SymbolTable {

    // ===== Symbol creation methods ======

    pub fn new_root(&mut self, entry_point: Rc<RefCell<EntryPoint>>) -> RootKey {
        let root_symbol = RootSymbol::new(entry_point);
        self.roots.insert(root_symbol)
    }
    // Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, parent: SymbolKey, name: &str, path: &str) -> FileKey {
        let is_external = self.is_external(parent);
        let file_symbol = FileSymbol::new(name, path, parent, is_external);
        let file_key = self.files.insert(file_symbol);
        self.add_to_parent_module_symbols(parent, file_key.into(), name, path);
        file_key
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, parent: SymbolKey, name: &str, path: &str, i_ext: &'static str) -> PythonPackageKey {
        let is_external = self.is_external(parent);
        let package_symbol = PythonPackageSymbol::new(name, path, parent, is_external, i_ext);
        let package_key = self.python_packages.insert(package_symbol);
        self.add_to_parent_module_symbols(parent, package_key.into(), name, path);
        package_key
    }

    pub fn add_new_namespace(&mut self, parent: SymbolKey, name: &str, path: &str) -> NamespaceKey {
        let is_external = self.is_external(parent);
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.add_to_parent_module_symbols(parent, namespace_key.into(), name, path);
        namespace_key
    }

    pub fn add_new_disk_dir(&mut self, parent: SymbolKey, name: &str, path: &str) -> DiskDirKey {
        let is_external = self.is_external(parent);
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.add_to_parent_module_symbols(parent, disk_dir_key.into(), name, path);
        disk_dir_key
    }

    pub fn add_new_compiled(&mut self, parent: SymbolKey, name: &str, path: &str) -> CompiledKey {
        let is_external = self.is_external(parent);
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key = self.compiled.insert(compiled_symbol);
        self.add_to_parent_module_symbols(parent, compiled_key.into(), name, path);
        compiled_key
    }

    pub fn add_new_module_package(session: &mut SessionInfo, parent: NamespaceKey, name: &str, path: &PathBuf) -> ModuleKey {
        let is_external = session.sync_odoo.symbol_table.is_external(parent.into());
        let module = ModuleSymbol::new(name, path, parent, is_external);
        let path_str = module.path.clone();
        let module_key = session.st_mut().modules.insert(module);
        ModuleSymbol::load_manifest_content(session, module_key);
        session.st_mut().add_to_parent_module_symbols(parent.into(), module_key.into(), name, &path_str);
        module_key
    }

    pub fn add_new_variable(&mut self, parent: impl Into<SymbolKey>, name: &str, range: TextRange) -> VariableKey {
        let parent = parent.into();
        let is_external = self.is_external(parent);
        let variable_symbol = VariableSymbol::new(name, parent, range, is_external);
        let variable_key = self.variables.insert(variable_symbol);
        self.add_to_parent_symbols(parent, variable_key.into(), name, range.start().to_u32());
        variable_key
    }
    pub fn add_new_function(&mut self, parent: SymbolKey, name: &str, range: TextRange, body_start: &TextSize) -> FunctionKey {
        let is_external = self.is_external(parent);
        let function_symbol = FunctionSymbol::new(name, parent, range, body_start.clone(), is_external);
        let function_key = self.functions.insert(function_symbol);
        self.add_to_parent_symbols(parent, function_key.into(), name, range.start().to_u32());
        function_key
    }

    pub fn add_new_class(&mut self, parent: SymbolKey, name: &str, range: TextRange, body_start: &TextSize) -> ClassKey {
        let is_external = self.is_external(parent);
        let class_symbol = ClassSymbol::new(name, parent, range, *body_start, is_external);
        let class_key = self.classes.insert(class_symbol);
        self.add_to_parent_symbols(parent, class_key.into(), name, range.start().to_u32());
        class_key
    }

    pub fn add_new_xml_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> XmlFileKey {
        let parent_symbol = &self.modules[parent];
        let mut xml_file_symbol = XmlFileSymbol::new(name, path, parent, parent_symbol.is_external);
        xml_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let xml_file_key = self.xml_files.insert(xml_file_symbol);
        self.add_to_module_data_symbols(parent, path, xml_file_key.into());
        xml_file_key
    }

    pub fn add_new_xml_record(&mut self, parent: SymbolKey, model: (OYarn, Range<usize>) , xml_id: Option<OYarn>, range: TextRange) -> XmlRecordKey {
        let is_external = self.is_external(parent);
        let xml_record_sym = XmlRecordSymbol::new(
            model,
            xml_id,
            range,
            parent,
            is_external);
        let xml_record_key = self.xml_records.insert(xml_record_sym);
        self.add_xml_data_to_file(parent, xml_record_key.into());
        xml_record_key
    }

    pub fn add_new_xml_menuitem(&mut self, parent: XmlFileKey, xml_id: Option<OYarn>, range: TextRange) -> XmlMenuItemKey {
        let is_external = self.is_external(parent.into());
        let xml_menuitem_sym = XmlMenuItemSymbol::new(xml_id, range, parent.into(), is_external);
        let xml_menuitem_key = self.xml_menuitems.insert(xml_menuitem_sym);
        self.add_xml_data_to_file(parent.into(), xml_menuitem_key.into());
        xml_menuitem_key
    }

    pub fn add_new_xml_asset(&mut self, parent: XmlFileKey, xml_id: Option<OYarn>, range: TextRange) -> XmlAssetKey {
        let is_external = self.is_external(parent.into());
        let xml_asset_sym = XmlAssetSymbol::new(xml_id, range, parent.into(), is_external);
        let xml_asset_key = self.xml_assets.insert(xml_asset_sym);
        self.add_xml_data_to_file(parent.into(), xml_asset_key.into());
        xml_asset_key
    }

    pub fn add_new_xml_delete(&mut self, parent: XmlFileKey, xml_id: Option<OYarn>, range: TextRange, model: OYarn) -> XmlDeleteKey {
        let is_external = self.is_external(parent.into());
        let xml_delete_sym = XmlDeleteSymbol::new(xml_id, range, model, parent.into(), is_external);
        let xml_delete_key = self.xml_deletes.insert(xml_delete_sym);
        self.add_xml_data_to_file(parent.into(), xml_delete_key.into());
        xml_delete_key
    }

    //parent should be either XmlRecord or XmlAsset
    pub fn add_new_xml_field(&mut self, parent: SymbolKey, field_name: OYarn, range: TextRange, text: Option<String>, text_range: Option<TextRange>, ref_key: Option<(String, TextRange)>) -> XmlFieldKey {
        let is_external = self.is_external(parent);
        let xml_field_sym = XmlFieldSymbol::new(field_name.clone(), range, text, text_range, ref_key, parent, is_external);
        let xml_field_key = self.xml_fields.insert(xml_field_sym);
        self.add_field_to_xml_record(parent, xml_field_key, field_name.as_str());
        xml_field_key
    }

    pub fn add_new_xml_template(&mut self, parent: XmlFileKey, name: Option<OYarn>, t_name: Option<OYarn>, range: TextRange, is_web: bool) -> XmlTemplateKey {
        let is_external = self.is_external(parent.into());
        let xml_template_sym = XmlTemplateSymbol::new(name, t_name, range, parent.into(), is_web, is_external);
        let xml_template_key = self.xml_templates.insert(xml_template_sym);
        self.add_xml_data_to_file(parent.into(), xml_template_key.into());
        xml_template_key
    }

    pub fn add_new_csv_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> CsvFileKey {
        let parent_symbol = &self.modules[parent];
        let mut csv_file_symbol = CsvFileSymbol::new(name, path, parent, parent_symbol.is_external);
        csv_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let csv_file_key = self.csv_files.insert(csv_file_symbol);
        self.add_to_module_data_symbols(parent, path, csv_file_key.into());
        csv_file_key
    }

    pub fn add_new_js_file(&mut self, parent_key: JsFileParent, name: &str, path: &str) -> JsFileKey {
        let symbol_key: SymbolKey = parent_key.into();
        let mut js_file_symbol = JsFileSymbol::new(name, path, parent_key, self.is_external(symbol_key));
        js_file_symbol.set_in_workspace(self.in_workspace(symbol_key));
        let js_file_key = self.js_files.insert(js_file_symbol);
        let rc_entry = self.get_entry(symbol_key);
        let mut entry_bw = rc_entry.borrow_mut();
        self.add_to_parent_js_symbols(parent_key, path, js_file_key);
        self.add_to_js_entry_symbols(&mut entry_bw, path, js_file_key.into());
        js_file_key
    }

    pub fn add_new_ext_symbol(
        &mut self,
        target: SymbolKey,
        name: &str,
        range: TextRange,
        owner: SymbolKey,
    ) -> VariableKey {
        // validate target can host an external symbol
        if !matches!(target.typ(),
            SymType::FILE | SymType::PACKAGE(PackageType::MODULE)
                | SymType::PACKAGE(PackageType::PYTHON_PACKAGE)
                | SymType::CLASS | SymType::FUNCTION | SymType::NAMESPACE
        ) {
            panic!("Impossible to add an external symbol to a {}", target.typ());
        }
        let variable_symbol = VariableSymbol::new(
            name,
            target,
            range,
            self.is_external(target),
        );
        let variable_key = self.variables.insert(variable_symbol);
        let section = self.get_section_for_key(owner, range.start().to_u32());

        self.ext_symbols.add(target, owner, name, section, variable_key);
        variable_key
    }

    // ====== Helpers for symbol creation ======

    // If replacing an existing entry, the old one gets removed from the symbol table to prevent leaks,
    // but `unload` side effects are not run. Callers should properly unload the symbol first.
    fn add_to_parent_module_symbols(&mut self, parent: SymbolKey, child: SymbolKey, name: &str, path: &str) {
        let replaced_key = match parent {
            SymbolKey::Namespace(n) => {
                self.add_file_to_namespace(n, child, name, path)
            },
            SymbolKey::PythonPackage(p) => {
                self.python_packages[p].module_symbols.insert(oyarn!("{}", name), child)
            },
            SymbolKey::Module(m) => {
                self.modules[m].module_symbols.insert(oyarn!("{}", name), child)
            },
            SymbolKey::Root(r) => {
                self.roots[r].module_symbols.insert(oyarn!("{}", name), child)
            },
            SymbolKey::DiskDir(d) => {
                self.disk_dirs[d].module_symbols.insert(oyarn!("{}", name), child)
            },
            SymbolKey::Compiled(c) if child.typ() == SymType::COMPILED => {
                // A compiled can only be a parent to another compiled
                self.compiled[c].module_symbols.insert(oyarn!("{}", name), child)
            }
            _ => {
                panic!("Impossible to add a {} to a {}", child.typ(), parent.typ());
            }
        };
        if let Some(replaced_key) = replaced_key {
            self.remove(replaced_key);
        }
    }

    fn add_file_to_namespace(&mut self, parent: NamespaceKey, file: SymbolKey, name: &str, path: &str) -> Option<SymbolKey> {
        let ns = &mut self.namespaces[parent];
        let best = ns.directories.iter()
            .enumerate()
            .filter(|(_, dir)| PathBuf::from(path).starts_with(&dir.path))
            .max_by_key(|(_, dir)| dir.path.len())
            .unwrap_or_else(|| panic!("Not valid path found to add the file ({}) to namespace {} with directories {:?}", path, ns.name, ns.directories))
            .0;
        ns.directories[best].module_symbols.insert(oyarn!("{}", name), file)
    }

    fn add_field_to_xml_record(&mut self, parent: SymbolKey, field: XmlFieldKey, name: &str) {
        match parent {
            SymbolKey::XmlRecord(r) => {
                let xml_record = &mut self.xml_records[r];
                if let Some(replaced_key) = xml_record.fields.insert(oyarn!("{}", name), field) {
                    self.remove(replaced_key.into());
                }
            },
            SymbolKey::XmlAsset(k) => {
                let xml_asset = &mut self.xml_assets[k];
                if let Some(replaced_key) = xml_asset.fields.insert(oyarn!("{}", name), field) {
                    self.remove(replaced_key.into());
                }
            }
            _ => {
                panic!("Impossible to add an XmlFieldKey to a {}", parent.typ());
            }
        }
    }

    fn add_xml_data_to_file(&mut self, parent: SymbolKey, content: XmlDataKey) {
        match parent {
            SymbolKey::XmlFile(f) => {
                let xml_file = &mut self.xml_files[f];
                xml_file.symbols.insert(content);
            },
            SymbolKey::CsvFile(f) => {
                let csv_file = &mut self.csv_files[f];
                csv_file.symbols.insert(content);
            },
            _ => {
                panic!("Impossible to add an xml data key to a {}", parent.typ());
            }
        }
    }

    fn add_to_parent_symbols(&mut self, parent: SymbolKey, content: SymbolKey, name: &str, position: u32) {
         match parent {
            SymbolKey::File(f) => {
                let file = &mut self.files[f];
                let section = file.get_section_for(position).index;
                file.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::Module(m) => {
                let module = &mut self.modules[m];
                let section = module.get_section_for(position).index;
                module.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::PythonPackage(p) => {
                let package = &mut self.python_packages[p];
                let section = package.get_section_for(position).index;
                package.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::Class(c) => {
                let class = &mut self.classes[c];
                let section = class.get_section_for(position).index;
                class.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::Function(f) => {
                let function = &mut self.functions[f];
                let section = function.get_section_for(position).index;
                function.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            _ => {
                panic!("Impossible to add a {} to a {}", content.typ(), parent.typ());
            }
        }
    }

    // If replacing an exisiting entry, the old one gets removed from the symbol table to prevent leaks,
    // but `unload` side effects are not run. Callers should properly unload the symbol first.
    fn add_to_module_data_symbols(&mut self, parent: ModuleKey, path: &str, data_file: SourceFileKey) {
        let replaced_key = self.modules[parent].data_symbols.insert(path.to_string(), data_file);
        if let Some(replaced_key) = replaced_key {
            if self.is_key_valid(replaced_key) {
                self.remove(replaced_key.into());
            }
        }
    }

    fn add_to_parent_js_symbols(&mut self, parent: JsFileParent, path: &str, js_key: JsFileKey) {
        match parent {
            JsFileParent::Module(m) => {
                let module = &mut self.modules[m];
                let replaced_key = module.js_symbols.insert(path.to_string(), js_key);
                if let Some(replaced_key) = replaced_key {
                    if self.is_key_valid(replaced_key) {
                        self.remove(replaced_key.into());
                    }
                }
            },
            JsFileParent::DiskDir(d) => {
                let disk_dir = &mut self.disk_dirs[d];
                let replaced_key = disk_dir.js_symbols.insert(path.to_string(), js_key);
                if let Some(replaced_key) = replaced_key {
                    if self.is_key_valid(replaced_key) {
                        self.remove(replaced_key.into());
                    }
                }
            }
        }
    }

    fn add_to_js_entry_symbols(&mut self, entry: &mut EntryPoint, path: &str, js_file: JsFileKey) {
        entry.js_symbols.insert(path.to_string(), Wk::from(js_file));
    }

    /* used by add_new_ext_symbol. Do not call directly */
    fn get_section_for_key(&self, owner: SymbolKey, position: u32) -> u32 {
        match owner {
            SymbolKey::File(f) => self[f].get_section_for(position).index,
            SymbolKey::Module(m) => self[m].get_section_for(position).index,
            SymbolKey::PythonPackage(p) => self[p].get_section_for(position).index,
            SymbolKey::Class(c) => self[c].get_section_for(position).index,
            SymbolKey::Function(f) => self[f].get_section_for(position).index,
            _ => panic!("Impossible to add a declaration of external symbol to a {}", owner.typ()),
        }
    }

    // ====== Symbol removal ======

    /// Remove `symbol` and its descendants, and clean up.
    /// WARNING: After this call, `symbol` and its descendants are no longer valid keys.
    pub fn unload(session: &mut SessionInfo, symbol: SymbolKey) {
        fn unload_recursively(session: &mut SessionInfo, symbol: SymbolKey) {
            for child in session.st().children(symbol) {
                unload_recursively(session, child);
            }
            SyncOdoo::on_unload(session, symbol);
        }
        unload_recursively(session, symbol);
        session.st_mut().unlink_from_parent(symbol);
        session.st_mut().remove(symbol);
    }

    /// Only accessible from entry point module
    pub fn drop_root(&mut self, root: RootKey, _: EntryPointCleanupToken) {
        if self.roots.contains_key(root) {
            self.remove(root.into());
        }
    }

    // ===== Symbol removal helpers ======

    /// Remove symbol and descendants from the symbol table.
    fn remove(&mut self, key: SymbolKey) {
        for child in self.children(key) {
            self.remove(child);
        }
        for ext_var in self.ext_symbols.remove(key) {
            self.remove(ext_var.into());
        }
        match key {
            SymbolKey::Root(k) => { self.roots.remove(k); }
            SymbolKey::DiskDir(k) => { self.disk_dirs.remove(k); }
            SymbolKey::Namespace(k) => { self.namespaces.remove(k); }
            SymbolKey::PythonPackage(k) => { self.python_packages.remove(k); }
            SymbolKey::Module(k) => { self.modules.remove(k); }
            SymbolKey::File(k) => { self.files.remove(k); }
            SymbolKey::Compiled(k) => { self.compiled.remove(k); }
            SymbolKey::Class(k) => { self.classes.remove(k); }
            SymbolKey::Function(k) => { self.functions.remove(k); }
            SymbolKey::Variable(k) => { self.variables.remove(k); }
            SymbolKey::XmlFile(k) => { self.xml_files.remove(k); }
            SymbolKey::XmlRecord(k) => { self.xml_records.remove(k); }
            SymbolKey::XmlField(k) => { self.xml_fields.remove(k); }
            SymbolKey::XmlAsset(k) => { self.xml_assets.remove(k); }
            SymbolKey::XmlMenuItem(k) => { self.xml_menuitems.remove(k); }
            SymbolKey::XmlTemplate(k) => { self.xml_templates.remove(k); }
            SymbolKey::XmlDelete(k) => { self.xml_deletes.remove(k); }
            SymbolKey::CsvFile(k) => { self.csv_files.remove(k); }
            SymbolKey::JsFile(k) => { self.js_files.remove(k); }
        }
    }

    fn children(&self, key: SymbolKey) -> Vec<SymbolKey> {
        match key {
            SymbolKey::Root(r) => self[r].children(),
            SymbolKey::DiskDir(d) => self[d].children(),
            SymbolKey::Namespace(n) => self[n].children(),
            SymbolKey::Module(m) => self[m].children(),
            SymbolKey::PythonPackage(p) => self[p].children(),
            SymbolKey::File(f) => self[f].children(),
            SymbolKey::Compiled(c) => self[c].children(),
            SymbolKey::Class(c) => self[c].children(),
            SymbolKey::Function(f) => self[f].children(),
            SymbolKey::Variable(v) => self[v].children(),
            SymbolKey::XmlFile(x) => self[x].children(),
            SymbolKey::XmlRecord(x) => self[x].children(),
            SymbolKey::XmlField(x) => self[x].children(),
            SymbolKey::XmlAsset(x) => self[x].children(),
            SymbolKey::XmlMenuItem(x) => self[x].children(),
            SymbolKey::XmlTemplate(x) => self[x].children(),
            SymbolKey::XmlDelete(x) => self[x].children(),
            SymbolKey::CsvFile(c) => self[c].children(),
            SymbolKey::JsFile(j) => self[j].children(),
        }
    }

    fn unlink_from_parent(&mut self, child: SymbolKey) {
        let child_name = self.name(child).clone();
        let parent = self.parent(child).expect("symbol should have a parent");
        match parent {
            SymbolKey::Root(r) => { self.roots[r].module_symbols.remove(&child_name); },
            SymbolKey::DiskDir(d) => { self.disk_dirs[d].module_symbols.remove(&child_name); },
            SymbolKey::Namespace(n) => {
                for directory in self.namespaces[n].directories.iter_mut() {
                    directory.module_symbols.remove(&child_name);
                }
            },
            SymbolKey::Module(m) => match child {
                SymbolKey::XmlFile(x) => {
                    self.modules[m].data_symbols.remove(&self.xml_files[x].path);
                },
                SymbolKey::CsvFile(c) => {
                    self.modules[m].data_symbols.remove(&self.csv_files[c].path);
                },
                SymbolKey::JsFile(f) => {
                    self.modules[m].js_symbols.remove(&self.js_files[f].path);
                }
                _ => {
                    if self.is_file_content(child) {
                        self.modules[m].symbols.remove(&child_name);
                    } else {
                        self.modules[m].module_symbols.remove(&child_name);
                    }
                },
            },
            SymbolKey::PythonPackage(p) => {
                if self.is_file_content(child) {
                     self.python_packages[p].symbols.remove(&child_name);
                } else {
                    self.python_packages[p].module_symbols.remove(&child_name);
                }
            },
            SymbolKey::File(f) => { self.files[f].symbols.remove(&child_name); },
            SymbolKey::Compiled(c) => { self.compiled[c].module_symbols.remove(&child_name); },
            SymbolKey::Class(c) => { self.classes[c].symbols.remove(&child_name); },
            SymbolKey::Function(f) => { self.functions[f].symbols.remove(&child_name); },
            SymbolKey::Variable(_) => { panic!("A variable cannot be a parent") },
            SymbolKey::XmlFile(f) => { self.xml_files[f].symbols.remove(&child.as_xml_data_key().expect("Content of xmlfile should be an xml_data_key")); },
            SymbolKey::XmlRecord(r) => { self.xml_records[r].fields.remove(&child_name); },
            SymbolKey::XmlField(_) => { panic!("An XML field cannot be a parent") },
            SymbolKey::XmlAsset(a) => { self.xml_assets[a].fields.remove(&child_name); },
            SymbolKey::XmlMenuItem(_) => { panic!("An XML menu item cannot be a parent") },
            SymbolKey::XmlTemplate(_) => { panic!("An XML template cannot be a parent") },
            SymbolKey::XmlDelete(_) => { panic!("An XML delete cannot be a parent") },
            SymbolKey::CsvFile(_) => { panic!("A CSV file symbol cannot be a parent") },
            SymbolKey::JsFile(_) => { panic!("A JS file symbol cannot be a parent") },
        }
    }

}
