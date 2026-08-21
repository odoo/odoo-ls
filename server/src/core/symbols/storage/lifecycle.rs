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

#[derive(Debug)]
pub struct NameTakenError(pub SymbolKey);

impl SymbolTable {

    // ===== Symbol creation methods ======

    pub fn new_root(&mut self, entry_point: Rc<RefCell<EntryPoint>>) -> RootKey {
        let root_symbol = RootSymbol::new(entry_point);
        self.roots.insert(root_symbol)
    }
    // Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> Result<FileKey, NameTakenError> {
        self.check_fs_symbol_name_vacant(parent, name)?;
        let is_external = self.is_external(parent.into());
        let file_symbol = FileSymbol::new(name, path, parent, is_external);
        let file_key = self.files.insert(file_symbol);
        self.add_to_parent_fs_symbols(parent, file_key.into(), name, path);
        Ok(file_key)
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str, i_ext: &'static str) -> Result<PythonPackageKey, NameTakenError> {
        self.check_fs_symbol_name_vacant(parent, name)?;
        let is_external = self.is_external(parent.into());
        let package_symbol = PythonPackageSymbol::new(name, path, parent, is_external, i_ext);
        let package_key = self.python_packages.insert(package_symbol);
        self.add_to_parent_fs_symbols(parent, package_key.into(), name, path);
        Ok(package_key)
    }

    pub fn add_new_namespace(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> Result<NamespaceKey, NameTakenError> {
        self.check_fs_symbol_name_vacant(parent, name)?;
        let is_external = self.is_external(parent.into());
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.add_to_parent_fs_symbols(parent, namespace_key.into(), name, path);
        Ok(namespace_key)
    }

    pub fn add_new_disk_dir(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> Result<DiskDirKey, NameTakenError> {
        self.check_fs_symbol_name_vacant(parent, name)?;
        let is_external = self.is_external(parent.into());
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.add_to_parent_fs_symbols(parent, disk_dir_key.into(), name, path);
        Ok(disk_dir_key)
    }

    pub fn add_new_compiled(&mut self, parent: FileSystemSymbolParent, name: &str, path: &str) -> Result<CompiledKey, NameTakenError> {
        self.check_fs_symbol_name_vacant(parent, name)?;
        let is_external = self.is_external(parent.into());
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key = self.compiled.insert(compiled_symbol);
        self.add_to_parent_fs_symbols(parent, compiled_key.into(), name, path);
        Ok(compiled_key)
    }

    pub fn add_new_module_package(session: &mut SessionInfo, parent: NamespaceKey, name: &str, path: &Path) -> Result<ModuleKey, NameTakenError> {
        session.st().check_fs_symbol_name_vacant(parent.into(), name)?;
        let is_external = session.sync_odoo.symbol_table.is_external(parent.into());
        let module = ModuleSymbol::new(name, path, parent, is_external);
        let path_str = module.path.clone();
        let module_key = session.st_mut().modules.insert(module);
        session.st_mut().add_to_parent_fs_symbols(parent.into(), module_key.into(), name, &path_str);
        ModuleSymbol::load_manifest_content(session, module_key);
        Ok(module_key)
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

    pub fn add_new_xml_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> Result<XmlFileKey, NameTakenError> {
        self.check_data_symbol_path_vacant(parent, path)?;
        let parent_symbol = &self.modules[parent];
        let mut xml_file_symbol = XmlFileSymbol::new(name, path, parent, parent_symbol.is_external);
        xml_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let xml_file_key = self.xml_files.insert(xml_file_symbol);
        self.add_to_module_data_symbols(parent, path, xml_file_key.into());
        Ok(xml_file_key)
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

    pub fn add_new_xml_field(
        &mut self,
        parent: XmlFieldParent,
        field_name: &str,
        range: TextRange,
        text: Option<String>,
        text_range: Option<TextRange>,
        ref_key: Option<(String, TextRange)>,
    ) -> Result<XmlFieldKey, NameTakenError> {
        self.check_xml_field_name_vacant(parent, field_name)?;
        let is_external = self.is_external(parent.into());
        let xml_field_sym = XmlFieldSymbol::new(field_name, range, text, text_range, ref_key, parent, is_external);
        let xml_field_key = self.xml_fields.insert(xml_field_sym);
        self.add_field_to_xml_record(parent, xml_field_key, field_name);
        Ok(xml_field_key)
    }

    pub fn add_new_xml_template(&mut self, parent: XmlFileKey, name: Option<OYarn>, t_name: Option<OYarn>, range: TextRange, is_web: bool) -> XmlTemplateKey {
        let is_external = self.is_external(parent.into());
        let xml_template_sym = XmlTemplateSymbol::new(name, t_name, range, parent.into(), is_web, is_external);
        let xml_template_key = self.xml_templates.insert(xml_template_sym);
        self[parent].data_symbols.insert(xml_template_key.into());
        xml_template_key
    }

    pub fn add_new_csv_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> Result<CsvFileKey, NameTakenError> {
        self.check_data_symbol_path_vacant(parent, path)?;
        let parent_symbol = &self.modules[parent];
        let mut csv_file_symbol = CsvFileSymbol::new(name, path, parent, parent_symbol.is_external);
        csv_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let csv_file_key = self.csv_files.insert(csv_file_symbol);
        self.add_to_module_data_symbols(parent, path, csv_file_key.into());
        Ok(csv_file_key)
    }

    pub fn add_new_js_file(&mut self, parent: JsFileParent, name: &str, path: &str) -> Result<JsFileKey, NameTakenError> {
        self.check_js_symbol_path_vacant(parent, name)?;
        let mut js_file_symbol = JsFileSymbol::new(name, path, parent, self.is_external(parent.into()));
        js_file_symbol.set_in_workspace(self.in_workspace(parent.into()));
        let js_file_key = self.js_files.insert(js_file_symbol);
        self.add_to_parent_js_symbols(parent, path, js_file_key);
        ModuleSymbol::on_js_file_load(self, js_file_key);
        Ok(js_file_key)
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
    fn panic_on_replaced_key(replaced: Option<impl Into<SymbolKey>>) {
        if replaced.is_some() { panic!("replaced key on insertion") }
    }

    fn check_fs_symbol_name_vacant(&self, parent: FileSystemSymbolParent, name: &str) -> Result<(), NameTakenError> {
        match parent.get_child(self, name) {
            Some(existing) => Err(NameTakenError(existing)),
            None => Ok(())
        }
    }

    fn check_data_symbol_path_vacant(&self, parent: ModuleKey, path: &str) -> Result<(), NameTakenError> {
        match self[parent].data_file_symbols.get(path).copied() {
            Some(existing) => Err(NameTakenError(existing.into())),
            None => Ok(())
        }
    }
 
    fn check_js_symbol_path_vacant(&self, parent: JsFileParent, name: &str) -> Result<(), NameTakenError> {
        match parent.js_symbols(self).get(name).copied() {
            Some(existing) => Err(NameTakenError(existing.into())),
            None => Ok(())
        }
    }

    fn check_xml_field_name_vacant(&self, parent: XmlFieldParent, name: &str) -> Result<(), NameTakenError> {
        match parent.fields(self).get(name).copied() {
            Some(existing) => Err(NameTakenError(existing.into())),
            None => Ok(())
        }
    }

    fn add_to_parent_fs_symbols(&mut self, parent: FileSystemSymbolParent, child: SymbolKey, name: &str, path: &str) {
        // A compiled can only be a parent to another compiled
        if let FileSystemSymbolParent::Compiled(_) = parent && !matches!(child, SymbolKey::Compiled(_)) {
            panic!("Impossible to add a {} to a CompiledSymbol parent", child.typ());
        }
        let replaced_key = parent.add_fs_symbol(self, name, child, path);
        Self::panic_on_replaced_key(replaced_key);
    }

    fn remove_from_parent_fs_symbols(&mut self, parent: FileSystemSymbolParent, name: &str) {
        parent.remove_fs_symbol(self, name);
    }

    fn add_field_to_xml_record(&mut self, parent: XmlFieldParent, field: XmlFieldKey, name: &str) {
        let replaced_key = parent.fields_mut(self).insert(oyarn!("{}", name), field);
        Self::panic_on_replaced_key(replaced_key);
    }

    fn add_to_module_data_symbols(&mut self, parent: ModuleKey, path: &str, data_file: SourceFileKey) {
        let replaced_key = self.modules[parent].data_file_symbols.insert(path.to_string(), data_file);
        Self::panic_on_replaced_key(replaced_key);
    }
    
    fn remove_from_module_data_symbols(&mut self, parent: ModuleKey, path: &str) {
        self[parent].data_file_symbols.remove(path);
    }

    fn add_to_parent_js_symbols(&mut self, parent: JsFileParent, path: &str, js_key: JsFileKey) {
        let replaced_key = parent.js_symbols_mut(self).insert(path.to_string(), js_key);
        Self::panic_on_replaced_key(replaced_key);
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
                self.remove_from_parent_fs_symbols(parent, &name);
            },
            SourceFileKey::Module(m) => {
                let module_symbol = &self[m];
                let name = module_symbol.name.to_string();
                let parent = module_symbol.parent();
                self.remove_from_parent_fs_symbols(parent.into(), &name);
            },
            SourceFileKey::PythonPackage(p) => {
                let package_symbol = &self[p];
                let name = package_symbol.name.to_string();
                let parent = package_symbol.parent();
                self.remove_from_parent_fs_symbols(parent, &name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entry_point::EntryPointType;
    use crate::core::symbols::symbol_keys::KeyValidator;
    use crate::oyarn;

    fn range_at(start: u32) -> TextRange {
        TextRange::new(TextSize::new(start), TextSize::new(start + 1))
    }

    /// Parent-linking half of `add_new_module_package`. The other half,
    /// `load_manifest_content`, needs a `SessionInfo` and reads the manifest from disk.
    fn add_module(st: &mut SymbolTable, parent: NamespaceKey, name: &str, path: &str) -> ModuleKey {
        let is_external = st.is_external(parent.into());
        let module = ModuleSymbol::new(name, Path::new(path), parent, is_external);
        let module_key = st.modules.insert(module);
        st.add_to_parent_fs_symbols(parent.into(), module_key.into(), name, path);
        module_key
    }

    /// One symbol of every kind used as a parent below. `NamespaceSymbol` files its children by
    /// path, so everything given `namespace` as parent must live under `/root/ns`.
    struct Fixture {
        st: SymbolTable,
        root: RootKey,
        namespace: NamespaceKey,
        disk_dir: DiskDirKey,
        module: ModuleKey,
    }

    impl Fixture {
        fn new() -> Self {
            let mut st = SymbolTable::new();
            let entry = EntryPoint::new(&mut st, "/root".to_string(), vec![], EntryPointType::MAIN, None, None);
            let root = entry.borrow().root;
            let namespace = st.add_new_namespace(root.into(), "ns", "/root/ns").unwrap();
            let disk_dir = st.add_new_disk_dir(root.into(), "dd", "/root/dd").unwrap();
            let module = add_module(&mut st, namespace, "mod", "/root/ns/mod");
            Self { st, root, namespace, disk_dir, module }
        }

        fn holds(&mut self, parent: SymbolKey, child: impl Into<SymbolKey>) -> bool {
            self.st.children(parent).contains(&child.into())
        }
    }

    #[test]
    fn source_files_round_trip_through_their_parent() -> Result<(), NameTakenError> {
        let mut f = Fixture::new();
        // One child per `SourceFileKey` variant. `File` and `JsFile` appear twice, to cover both
        // of their parent kinds; the namespace is the only hand-written `FileSystemItemHolder`.
        let in_root = f.st.add_new_file(f.root.into(), "in_root", "/root/in_root.py")?;
        let in_namespace = f.st.add_new_file(f.namespace.into(), "in_namespace", "/root/ns/in_namespace.py")?;
        let package = f.st.add_new_python_package(f.root.into(), "a_package", "/root/a_package", "")?;
        let module = add_module(&mut f.st, f.namespace, "another_module", "/root/ns/another_module");
        let xml_file = f.st.add_new_xml_file(f.module, "data.xml", "/root/ns/mod/data.xml")?;
        let csv_file = f.st.add_new_csv_file(f.module, "res.partner.csv", "/root/ns/mod/res.partner.csv")?;
        let js_in_module = f.st.add_new_js_file(JsFileParent::Module(f.module), "widget.js", "/root/ns/mod/static/src/widget.js")?;
        let js_in_disk_dir = f.st.add_new_js_file(JsFileParent::DiskDir(f.disk_dir), "lib.js", "/root/dd/lib.js")?;
        let sibling = f.st.add_new_file(f.module.into(), "sibling", "/root/ns/mod/sibling.py")?;

        let cases: Vec<(SymbolKey, SourceFileKey)> = vec![
            (f.root.into(), in_root.into()),
            (f.namespace.into(), in_namespace.into()),
            (f.root.into(), package.into()),
            (f.namespace.into(), module.into()),
            (f.module.into(), xml_file.into()),
            (f.module.into(), csv_file.into()),
            (f.module.into(), js_in_module.into()),
            (f.disk_dir.into(), js_in_disk_dir.into()),
        ];

        for (parent, child) in cases {
            assert!(f.holds(parent, child), "{child:?} is not a child of {parent:?}");
            assert_eq!(f.st.parent(child), Some(parent), "{child:?} does not point back at {parent:?}");

            f.st.unlink_from_parent(child);
            assert!(!f.holds(parent, child), "{child:?} is still a child of {parent:?} after unlink");
        }

        assert!(f.holds(f.module.into(), sibling), "an unlink evicted an unrelated sibling");
        Ok(())
    }

    #[test]
    fn file_content_round_trips_and_dies_with_its_file() {
        let mut f = Fixture::new();
        let file = f.st.add_new_file(f.module.into(), "models", "/root/ns/mod/models.py").unwrap();
        let class = f.st.add_new_class(file.into(), "AClass", range_at(0), TextSize::new(0));
        let method = f.st.add_new_function(class.into(), "method", range_at(10), TextSize::new(10));
        let local = f.st.add_new_variable(method, "local", range_at(20));

        assert!(f.holds(file.into(), class));
        assert!(f.holds(class.into(), method));
        assert!(f.holds(method.into(), local));
        assert_eq!(f.st.parent(local), Some(SymbolKey::from(method)));

        // what `unload` does, minus `SyncOdoo::on_unload`
        f.st.unlink_from_parent(file.into());
        f.st.remove(file.into());

        assert!(!f.holds(f.module.into(), file));
        for key in [SymbolKey::from(file), class.into(), method.into(), local.into()] {
            assert!(!f.st.is_key_valid(key), "{key:?} outlived its file");
        }
        assert!(f.st.is_key_valid(f.module), "removing a file took its module down");
        f.st.assert_no_orphans();
    }

    #[test]
    fn xml_data_round_trips_and_dies_with_its_file() {
        let mut f = Fixture::new();
        let xml_file = f.st.add_new_xml_file(f.module, "data.xml", "/root/ns/mod/data.xml").unwrap();
        let csv_file = f.st.add_new_csv_file(f.module, "res.partner.csv", "/root/ns/mod/res.partner.csv").unwrap();

        let record = f.st.add_new_xml_record(XmlDataParent::XmlFile(xml_file), (oyarn!("res.partner"), 0..1), Some(oyarn!("a_partner")), range_at(0));
        let menuitem = f.st.add_new_xml_menuitem(xml_file, Some(oyarn!("a_menu")), range_at(10));
        let field = f.st.add_new_xml_field(XmlFieldParent::XmlRecord(record), "name", range_at(1), None, None, None).unwrap();
        let csv_record = f.st.add_new_xml_record(XmlDataParent::CsvFile(csv_file), (oyarn!("res.partner"), 0..1), Some(oyarn!("a_csv_partner")), range_at(0));

        assert!(f.holds(xml_file.into(), record));
        assert!(f.holds(xml_file.into(), menuitem));
        assert!(f.holds(record.into(), field));
        assert!(f.holds(csv_file.into(), csv_record));
        assert_eq!(f.st.parent(field), Some(SymbolKey::from(record)));

        f.st.unlink_from_parent(xml_file.into());
        f.st.remove(xml_file.into());

        assert!(!f.holds(f.module.into(), xml_file));
        for key in [SymbolKey::from(record), menuitem.into(), field.into()] {
            assert!(!f.st.is_key_valid(key), "{key:?} outlived its xml file");
        }
        assert!(f.st.is_key_valid(csv_record), "removing the xml file took the csv records down");
        f.st.assert_no_orphans();
    }

    /// Every family at once, plus the ext symbols hanging off the module's files — the one
    /// child-like relation `children()` does not cover. Removing the module has to leave
    /// nothing behind.
    #[test]
    fn unloading_a_module_leaves_no_orphan() -> Result<(), NameTakenError> {
        let mut f = Fixture::new();
        let file = f.st.add_new_file(f.module.into(), "models", "/root/ns/mod/models.py")?;
        let class = f.st.add_new_class(file.into(), "AClass", range_at(0), TextSize::new(0));
        let method = f.st.add_new_function(class.into(), "method", range_at(10), TextSize::new(10));
        f.st.add_new_variable(method, "local", range_at(20));
        f.st.add_new_variable(f.module, "MODULE_CONSTANT", range_at(0));

        let xml_file = f.st.add_new_xml_file(f.module, "data.xml", "/root/ns/mod/data.xml")?;
        let record = f.st.add_new_xml_record(XmlDataParent::XmlFile(xml_file), (oyarn!("res.partner"), 0..1), None, range_at(0));
        _ = f.st.add_new_xml_field(XmlFieldParent::XmlRecord(record), "name", range_at(1), None, None, None);
        let csv_file = f.st.add_new_csv_file(f.module, "res.partner.csv", "/root/ns/mod/res.partner.csv")?;
        f.st.add_new_xml_record(XmlDataParent::CsvFile(csv_file), (oyarn!("res.partner"), 0..1), None, range_at(0));
        _ = f.st.add_new_js_file(JsFileParent::Module(f.module), "widget.js", "/root/ns/mod/static/src/widget.js");

        // injected into a file outside the module, but owned by one of its files: it dies with
        // the owner, through `ext_symbols` rather than through any holder.
        let host = f.st.add_new_file(f.root.into(), "host", "/root/host.py")?;
        let injected = f.st.add_new_ext_symbol(host.into(), "injected", range_at(30), file.into());

        f.st.assert_no_orphans();

        f.st.unlink_from_parent(f.module.into());
        f.st.remove(f.module.into());

        assert!(!f.st.is_key_valid(f.module), "the module survived its own removal");
        assert!(!f.st.is_key_valid(injected), "the ext symbol outlived its owner");
        assert!(f.st.is_key_valid(host), "removing the module took the injection target down");
        f.st.assert_no_orphans();
        Ok(())
    }

    /// `ModuleSymbol` implements four holder traits, which is why `children` is a sequence of
    /// `if let`s over the parent enums instead of a match.
    #[test]
    fn module_children_join_every_family_it_holds() {
        let mut f = Fixture::new();
        let file = f.st.add_new_file(f.module.into(), "models", "/root/ns/mod/models.py").unwrap();
        let constant = f.st.add_new_variable(f.module, "MODULE_CONSTANT", range_at(0));
        let xml_file = f.st.add_new_xml_file(f.module, "data.xml", "/root/ns/mod/data.xml").unwrap();
        let csv_file = f.st.add_new_csv_file(f.module, "res.partner.csv", "/root/ns/mod/res.partner.csv").unwrap();
        let js_file = f.st.add_new_js_file(JsFileParent::Module(f.module), "widget.js", "/root/ns/mod/static/src/widget.js").unwrap();

        let children = f.st.children(f.module.into());
        for expected in [SymbolKey::from(file), constant.into(), xml_file.into(), csv_file.into(), js_file.into()] {
            assert!(children.contains(&expected), "{expected:?} missing from the module's children");
            assert_eq!(f.st.parent(expected), Some(SymbolKey::from(f.module)));
        }
        assert_eq!(children.len(), 5, "the module reports children it was not given: {children:?}");
    }
}
