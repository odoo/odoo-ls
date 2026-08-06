//! Symbol creation and destruction.

//! All slotmap insertions/removals and parent/child relationship mutations are
//! centralized here.
//!
//! The `symbols` and `module_symbols` fields on variant structs
//! are `pub(super)`, so only code within the storage module can mutate them.
//! Combined with private slotmaps, this guarantees that `parent`, `symbols`, and
//! `module_symbols` always hold valid keys — they can be trusted without validity
//! checks, unlike keys stored elsewhere (as Weak).

use crate::{
    constants::{OYarn, PackageType, SymType}, core::{
        entry_point::{EntryPoint, EntryPointCleanupToken},
        odoo::SyncOdoo,
        symbols::{
            ClassSymbol, CompiledSymbol, CsvFileSymbol, Dependencies, DiskDirSymbol, FileSymbol, FunctionSymbol, JsFileSymbol, ModuleSymbol, NamespaceSymbol, PythonPackageSymbol, RootSymbol, SymbolTable, VariableSymbol, XmlFileSymbol, storage::{
                FileContentParent, FileSystemSymbolParent, JsFileParent, XmlDataParent, XmlFieldParent, xml::{
                    xml_asset_symbol::XmlAssetSymbol, xml_delete_symbol::XmlDeleteSymbol,
                    xml_field_symbol::XmlFieldSymbol, xml_menuitem_symbol::XmlMenuItemSymbol,
                    xml_record_symbol::XmlRecordSymbol, xml_template_symbol::XmlTemplateSymbol,
                }
            }, symbol_keys::{
                ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, JsFileKey,
                ModuleKey, NamespaceKey, PythonPackageKey, RootKey, SourceFileKey, SymbolKey,
                VariableKey, XmlAssetKey, XmlDeleteKey, XmlFieldKey, XmlFileKey,
                XmlMenuItemKey, XmlRecordKey, XmlTemplateKey,
            }
        },
    }, oyarn, threads::SessionInfo
};
use ruff_text_size::{TextRange, TextSize};
use std::{cell::RefCell, ops::Range, path::Path, rc::Rc};

impl SymbolTable {

    // ===== Symbol creation methods ======

    pub fn new_root(&mut self, entry_point: Rc<RefCell<EntryPoint>>) -> RootKey {
        let root_symbol = RootSymbol::new(entry_point);
        self.roots.insert(root_symbol)
    }
    // Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> FileKey {
        let is_external = self.is_external(parent.into());
        let file_symbol = FileSymbol::new(name, path, parent, is_external);
        let file_key = self.files.insert(file_symbol);
        self.add_to_parent_module_symbols(parent, file_key.into(), name, path);
        file_key
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str, i_ext: &'static str) -> PythonPackageKey {
        let is_external = self.is_external(parent.into());
        let package_symbol = PythonPackageSymbol::new(name, path, parent, is_external, i_ext);
        let package_key = self.python_packages.insert(package_symbol);
        self.add_to_parent_module_symbols(parent, package_key.into(), name, path);
        package_key
    }

    pub fn add_new_namespace(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> NamespaceKey {
        let is_external = self.is_external(parent.into());
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.add_to_parent_module_symbols(parent, namespace_key.into(), name, path);
        namespace_key
    }

    pub fn add_new_disk_dir(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> DiskDirKey {
        let is_external = self.is_external(parent.into());
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.add_to_parent_module_symbols(parent, disk_dir_key.into(), name, path);
        disk_dir_key
    }

    pub fn add_new_compiled(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> CompiledKey {
        let is_external = self.is_external(parent.into());
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key = self.compiled.insert(compiled_symbol);
        self.add_to_parent_module_symbols(parent, compiled_key.into(), name, path);
        compiled_key
    }

    pub fn add_new_module_package(session: &mut SessionInfo, parent: NamespaceKey, name: &str, path: &Path) -> ModuleKey {
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
        let parent = FileContentParent::try_from(parent).expect("parent should be FileContentParent");
        let variable_symbol = VariableSymbol::new(name, parent, range, is_external);
        let variable_key = self.variables.insert(variable_symbol);
        parent.add_child(self, name, variable_key.into(), range.start().to_u32());
        variable_key
    }
    pub fn add_new_function(&mut self, parent: SymbolKey, name: &str, range: TextRange, body_start: TextSize) -> FunctionKey {
        let is_external = self.is_external(parent);
        let parent = FileContentParent::try_from(parent).expect("parent should be FileContentParent");
        let function_symbol = FunctionSymbol::new(name, parent, range, body_start, is_external);
        let function_key = self.functions.insert(function_symbol);
        parent.add_child(self, name, function_key.into(), range.start().to_u32());
        function_key
    }

    pub fn add_new_class(&mut self, parent: SymbolKey, name: &str, range: TextRange, body_start: TextSize) -> ClassKey {
        let is_external = self.is_external(parent);
        let parent = FileContentParent::try_from(parent).expect("parent should be FileContentParent");
        let class_symbol = ClassSymbol::new(name, parent, range, body_start, is_external);
        let class_key = self.classes.insert(class_symbol);
        parent.add_child(self, name, class_key.into(), range.start().to_u32());
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

    pub fn add_new_xml_record(&mut self, parent: XmlDataParent, model: (OYarn, Range<usize>) , xml_id: Option<OYarn>, range: TextRange) -> XmlRecordKey {
        let is_external = self.is_external(parent.into());
        let xml_record_sym = XmlRecordSymbol::new(
            model,
            xml_id,
            range,
            parent,
            is_external);
        let xml_record_key = self.xml_records.insert(xml_record_sym);
        parent.data_symbols_mut(self).insert(xml_record_key.into());
        xml_record_key
    }

    pub fn add_new_xml_menuitem(&mut self, parent: XmlFileKey, xml_id: Option<OYarn>, range: TextRange) -> XmlMenuItemKey {
        let is_external = self.is_external(parent.into());
        let xml_menuitem_sym = XmlMenuItemSymbol::new(xml_id, range, parent.into(), is_external);
        let xml_menuitem_key = self.xml_menuitems.insert(xml_menuitem_sym);
        self[parent].data_symbols.insert(xml_menuitem_key.into());
        xml_menuitem_key
    }

    pub fn add_new_xml_asset(&mut self, parent: XmlFileKey, xml_id: Option<OYarn>, range: TextRange) -> XmlAssetKey {
        let is_external = self.is_external(parent.into());
        let xml_asset_sym = XmlAssetSymbol::new(xml_id, range, parent.into(), is_external);
        let xml_asset_key = self.xml_assets.insert(xml_asset_sym);
        self[parent].data_symbols.insert(xml_asset_key.into());
        xml_asset_key
    }

    pub fn add_new_xml_delete(&mut self, parent: XmlFileKey, xml_id: Option<OYarn>, range: TextRange, model: OYarn) -> XmlDeleteKey {
        let is_external = self.is_external(parent.into());
        let xml_delete_sym = XmlDeleteSymbol::new(xml_id, range, model, parent.into(), is_external);
        let xml_delete_key = self.xml_deletes.insert(xml_delete_sym);
        self[parent].data_symbols.insert(xml_delete_key.into());
        xml_delete_key
    }

    pub fn add_new_xml_field(&mut self, parent: XmlFieldParent, field_name: OYarn, range: TextRange, text: Option<String>, text_range: Option<TextRange>, ref_key: Option<(String, TextRange)>) -> XmlFieldKey {
        let is_external = self.is_external(parent.into());
        let xml_field_sym = XmlFieldSymbol::new(field_name.clone(), range, text, text_range, ref_key, parent, is_external);
        let xml_field_key = self.xml_fields.insert(xml_field_sym);
        self.add_field_to_xml_record(parent, xml_field_key, field_name.as_str());
        xml_field_key
    }

    pub fn add_new_xml_template(&mut self, parent: XmlFileKey, name: Option<OYarn>, t_name: Option<OYarn>, range: TextRange, is_web: bool) -> XmlTemplateKey {
        let is_external = self.is_external(parent.into());
        let xml_template_sym = XmlTemplateSymbol::new(name, t_name, range, parent.into(), is_web, is_external);
        let xml_template_key = self.xml_templates.insert(xml_template_sym);
        self[parent].data_symbols.insert(xml_template_key.into());
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

    pub fn add_new_js_file(&mut self, parent: JsFileParent, name: &str, path: &str) -> JsFileKey {
        let mut js_file_symbol = JsFileSymbol::new(name, path, parent, self.is_external(parent.into()));
        js_file_symbol.set_in_workspace(self.in_workspace(parent.into()));
        let js_file_key = self.js_files.insert(js_file_symbol);
        self.add_to_parent_js_symbols(parent, path, js_file_key);
        ModuleSymbol::on_js_file_load(self, js_file_key);
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
        let parent = FileContentParent::try_from(owner).expect("ext symbol owner should be file-content parent");
        let variable_symbol = VariableSymbol::new(
            name,
            parent,
            range,
            self.is_external(target),
        );
        let variable_key = self.variables.insert(variable_symbol);
        let section = parent.as_symbol_mgr(self).get_section_for(range.start().to_u32()).index;

        self.ext_symbols.add(target, owner, name, section, variable_key);
        variable_key
    }

    // ====== Helpers for symbol creation ======
    /// Evict a child displaced by a map insert with a colliding name/path. This
    /// prevents a leak, but `unload` side effects are NOT run. Callers should
    /// properly unload the symbol first.
    fn remove_replaced(&mut self, replaced: Option<impl Into<SymbolKey>>) {
        if let Some(replaced) = replaced {
            self.remove(replaced.into());
        }
    }
    
    fn add_to_parent_module_symbols(&mut self, parent: FileSystemSymbolParent, child: SymbolKey, name: &str, path: &str) {
        // A compiled can only be a parent to another compiled
        if let FileSystemSymbolParent::Compiled(_) = parent && !matches!(child, SymbolKey::Compiled(_)) {
            panic!("Impossible to add a {} to a CompiledSymbol parent", child.typ());
        }
        let replaced_key = parent.add_fs_symbol(self, name, child, path);
        self.remove_replaced(replaced_key);
    }

    fn remove_from_parent_module_symbols(&mut self, parent: FileSystemSymbolParent, name: &str) {
        parent.remove_fs_symbol(self, name);
    }

    fn add_field_to_xml_record(&mut self, parent: XmlFieldParent, field: XmlFieldKey, name: &str) {
        let replaced_key = parent.fields_mut(self).insert(oyarn!("{}", name), field);
        self.remove_replaced(replaced_key);
    }

    fn add_to_module_data_symbols(&mut self, parent: ModuleKey, path: &str, data_file: SourceFileKey) {
        let replaced_key = self.modules[parent].data_file_symbols.insert(path.to_string(), data_file);
        self.remove_replaced(replaced_key);
    }
    
    fn remove_from_module_data_symbols(&mut self, parent: ModuleKey, path: &str) {
        self[parent].data_file_symbols.remove(path);
    }

    fn add_to_parent_js_symbols(&mut self, parent: JsFileParent, path: &str, js_key: JsFileKey) {
        let replaced_key = parent.js_symbols_mut(self).insert(path.to_string(), js_key);
        self.remove_replaced(replaced_key);
    }

    fn remove_from_parent_js_symbols(&mut self, parent: JsFileParent, path: &str) {
        parent.js_symbols_mut(self).remove(path);
    }
    // ====== Symbol removal ======

    /// Remove `symbol` and its descendants, and clean up.
    /// WARNING: After this call, `symbol` and its descendants are no longer valid keys.
    pub fn unload(session: &mut SessionInfo, symbol: SourceFileKey) {
        fn unload_recursively(session: &mut SessionInfo, symbol: SymbolKey) {
            for child in session.st().children(symbol) {
                unload_recursively(session, child);
            }
            SyncOdoo::on_unload(session, symbol);
        }
        unload_recursively(session, symbol.into());
        session.st_mut().unlink_from_parent(symbol);
        session.st_mut().remove(symbol.into());
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

    fn children(&self, key:SymbolKey) -> Vec<SymbolKey> {
        let mut result = vec![];
        if let Ok(parent) =  <FileContentParent>::try_from(key){
            result.extend(parent.children(self));
        }
        if let Ok(parent) = <FileSystemSymbolParent>::try_from(key){
            result.extend(parent.children(self));
        }
        // Data file parent: Module only
        if let SymbolKey::Module(module_key) = key {
            result.extend(self[module_key].data_file_symbols().values().copied().map(SymbolKey::from));
        }
        if let Ok(parent) = <JsFileParent>::try_from(key){
            result.extend(parent.js_symbols(self).values().copied().map(SymbolKey::from));
        }
        if let Ok(parent) = <XmlDataParent>::try_from(key){
            result.extend(parent.data_symbols(self).iter().copied().map(SymbolKey::from));
        }
        if let Ok(parent) = <XmlFieldParent>::try_from(key) {
            result.extend(parent.fields(self).values().copied().map(SymbolKey::from));
        }
        result
    }

    fn unlink_from_parent(&mut self, child: SourceFileKey) {
        match child {
            SourceFileKey::File(f) => {
                let file_symbol = &self[f];
                let name = file_symbol.name.to_string();
                let parent = file_symbol.parent();
                self.remove_from_parent_module_symbols(parent, &name);
            },
            SourceFileKey::Module(m) => {
                let module_symbol = &self[m];
                let name = module_symbol.name.to_string();
                let parent = module_symbol.parent();
                self.remove_from_parent_module_symbols(parent.into(), &name);
            },
            SourceFileKey::PythonPackage(p) => {
                let package_symbol = &self[p];
                let name = package_symbol.name.to_string();
                let parent = package_symbol.parent();
                self.remove_from_parent_module_symbols(parent, &name);
            },
            SourceFileKey::XmlFile(x) => {
                let xml_symbol = &self[x];
                let parent = xml_symbol.parent();
                let path = xml_symbol.path.clone();
                self.remove_from_module_data_symbols(parent, &path);
            },
            SourceFileKey::CsvFile(c) => {
                let csv_symbol = &self[c];
                let parent = csv_symbol.parent();
                let path = csv_symbol.path.clone();
                self.remove_from_module_data_symbols(parent, &path);
            },
            SourceFileKey::JsFile(j) => {
                let js_symbol = &self[j];
                let parent = js_symbol.parent();
                let path = js_symbol.path.clone();
                self.remove_from_parent_js_symbols(parent, &path);
            },
        }
    }
}
