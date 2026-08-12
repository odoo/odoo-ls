pub mod buildable;
pub mod class_symbol;
pub mod compiled_symbol;
pub mod csv_file_symbol;
pub mod disk_dir_symbol;
pub mod file_symbol;
pub mod function_symbol;
pub mod js_file_symbol;
pub mod module_symbol;
pub mod namespace_symbol;
pub mod package_symbol;
pub mod root_symbol;
pub mod variable_symbol;
pub mod xml;
pub mod lifecycle;
pub mod symbol_mgr;
pub mod dependency_mgr;
pub mod metrics;
pub mod parents;
mod ext_symbol_store;

use crate::{constants::OYarn, core::{symbols::{
    ClassSymbol, CompiledSymbol, CsvFileSymbol, DiskDirSymbol, FileSymbol, FunctionSymbol, JsFileSymbol, ModuleSymbol, NamespaceSymbol, PythonPackageSymbol, RootSymbol, VariableSymbol, XmlFileSymbol, storage::xml::{xml_asset_symbol::XmlAssetSymbol, xml_delete_symbol::XmlDeleteSymbol, xml_field_symbol::XmlFieldSymbol, xml_menuitem_symbol::XmlMenuItemSymbol, xml_record_symbol::XmlRecordSymbol, xml_template_symbol::XmlTemplateSymbol}, symbol_keys::{
        ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, JsFileKey, KeyValidator, ModuleKey, NamespaceKey, PythonPackageKey, RootKey, VariableKey, XmlAssetKey, XmlDeleteKey, XmlFieldKey, XmlFileKey, XmlMenuItemKey, XmlRecordKey, XmlTemplateKey
    }
}}};
use duplicate::duplicate;
#[cfg(test)]
use duplicate::duplicate_item;
use ext_symbol_store::ExtSymbolStore;
use slotmap::{SlotMap, SparseSecondaryMap};
use std::ops::{Index, IndexMut};
pub use parents::{FileContentParent, FileSystemSymbolParent, JsFileParent, XmlDataParent, XmlFieldParent};

#[derive(Debug)]
pub struct SymbolTable {
    // slotmaps per symbol type
    roots: SlotMap<RootKey, RootSymbol>,
    disk_dirs: SlotMap<DiskDirKey, DiskDirSymbol>,
    namespaces: SlotMap<NamespaceKey, NamespaceSymbol>,
    python_packages: SlotMap<PythonPackageKey, PythonPackageSymbol>,
    modules: SlotMap<ModuleKey, ModuleSymbol>,
    files: SlotMap<FileKey, FileSymbol>,
    compiled: SlotMap<CompiledKey, CompiledSymbol>,
    classes: SlotMap<ClassKey, ClassSymbol>,
    functions: SlotMap<FunctionKey, FunctionSymbol>,
    variables: SlotMap<VariableKey, VariableSymbol>,
    xml_files: SlotMap<XmlFileKey, XmlFileSymbol>,
    csv_files: SlotMap<CsvFileKey, CsvFileSymbol>,
    xml_records: SlotMap<XmlRecordKey, XmlRecordSymbol>,
    xml_fields: SlotMap<XmlFieldKey, XmlFieldSymbol>,
    xml_menuitems: SlotMap<XmlMenuItemKey, XmlMenuItemSymbol>,
    xml_templates: SlotMap<XmlTemplateKey, XmlTemplateSymbol>,
    xml_assets: SlotMap<XmlAssetKey, XmlAssetSymbol>,
    xml_deletes: SlotMap<XmlDeleteKey, XmlDeleteSymbol>,
    js_files: SlotMap<JsFileKey, JsFileSymbol>,
    // external symbols
    ext_symbols: ExtSymbolStore,
    // secondary slotmaps
    xml_declared_models: SparseSecondaryMap<XmlRecordKey, OYarn>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            roots: SlotMap::with_key(),
            disk_dirs: SlotMap::with_key(),
            namespaces: SlotMap::with_key(),
            python_packages: SlotMap::with_key(),
            modules: SlotMap::with_key(),
            files: SlotMap::with_key(),
            compiled: SlotMap::with_key(),
            classes: SlotMap::with_key(),
            functions: SlotMap::with_key(),
            variables: SlotMap::with_key(),
            xml_files: SlotMap::with_key(),
            csv_files: SlotMap::with_key(),
            xml_records: SlotMap::with_key(),
            xml_fields: SlotMap::with_key(),
            xml_menuitems: SlotMap::with_key(),
            xml_templates: SlotMap::with_key(),
            xml_assets: SlotMap::with_key(),
            xml_deletes: SlotMap::with_key(),
            js_files: SlotMap::with_key(),
            ext_symbols: ExtSymbolStore::new(),
            xml_declared_models: SparseSecondaryMap::new(),
        }
    }

    pub fn pre_allocate(&mut self) {
        // Example code
        // self.files.reserve(7000);
        // self.packages.reserve(2200);
        // self.classes.reserve(14000);
        // self.functions.reserve(80000);
        // self.variables.reserve(450000);
        // self.xml_files.reserve(3200);
    }

    /// Iterate every XML template symbol — the only whole-set read access (the slotmap is
    /// private). Used by reverse lookups without a dedicated index (template-name refs).
    pub fn iter_xml_templates(&self) -> impl Iterator<Item = (XmlTemplateKey, &XmlTemplateSymbol)> {
        self.xml_templates.iter()
    }
}

/*
    Implement Index IndexMut and KeyValidator for each symbol type.
    Allows to access symbols like symbol_table[file_key].

    E.g.:

    let symbol_table: SymbolTable = ...;
    let file_key: FileKey = ...;
    let class_key: ClassKey = ...;

    let file_symbol: &FileSymbol = &symbol_table[file_key];
    let class_symbol: &ClassSymbol = &symbol_table[class_key];

    (type annotations not needed, it's here just to show the return type of the indexing operation)

    Indexing only works for the specific keys (FileKey, ClassKey, etc), not for the generic SymbolKey.
    You can use SymbolTable methods like name, parent, etc that operate on SymbolKey.
 */

duplicate!{
    [
        key output field;
        [RootKey] [RootSymbol] [roots];
        [DiskDirKey] [DiskDirSymbol] [disk_dirs];
        [NamespaceKey] [NamespaceSymbol] [namespaces];
        [PythonPackageKey] [PythonPackageSymbol] [python_packages];
        [ModuleKey] [ModuleSymbol] [modules];
        [FileKey] [FileSymbol] [files];
        [CompiledKey] [CompiledSymbol] [compiled];
        [ClassKey] [ClassSymbol] [classes];
        [FunctionKey] [FunctionSymbol] [functions];
        [VariableKey] [VariableSymbol] [variables];
        [XmlFileKey] [XmlFileSymbol] [xml_files];
        [XmlRecordKey] [XmlRecordSymbol] [xml_records];
        [XmlFieldKey] [XmlFieldSymbol] [xml_fields];
        [XmlMenuItemKey] [XmlMenuItemSymbol] [xml_menuitems];
        [XmlTemplateKey] [XmlTemplateSymbol] [xml_templates];
        [XmlAssetKey] [XmlAssetSymbol] [xml_assets];
        [XmlDeleteKey] [XmlDeleteSymbol] [xml_deletes];
        [CsvFileKey] [CsvFileSymbol] [csv_files];
        [JsFileKey] [JsFileSymbol] [js_files];
    ]
    impl Index<key> for SymbolTable {
        type Output = output;
        fn index(&self, k: key) -> &output {
            &self.field[k]
        }
    }

    impl IndexMut<key> for SymbolTable {
        fn index_mut(&mut self, k: key) -> &mut output {
            &mut self.field[k]
        }
    }

    impl KeyValidator<key> for SymbolTable {
        fn is_key_valid(&self, k: key) -> bool {
            self.field.contains_key(k)
        }
    }
}


#[cfg(test)]
#[duplicate_item(
    method_name                    field;
    [assert_no_orphans_in_roots]            [roots];
    [assert_no_orphans_in_disk_dirs]        [disk_dirs];
    [assert_no_orphans_in_namespaces]       [namespaces];
    [assert_no_orphans_in_python_packages]  [python_packages];
    [assert_no_orphans_in_modules]          [modules];
    [assert_no_orphans_in_files]            [files];
    [assert_no_orphans_in_compiled]         [compiled];
    [assert_no_orphans_in_classes]          [classes];
    [assert_no_orphans_in_functions]        [functions];
    [assert_no_orphans_in_variables]        [variables];
    [assert_no_orphans_in_xml_files]        [xml_files];
    [assert_no_orphans_in_csv_files]        [csv_files];
    [assert_no_orphans_in_xml_records]      [xml_records];
    [assert_no_orphans_in_xml_fields]       [xml_fields];
    [assert_no_orphans_in_xml_menuitems]    [xml_menuitems];
    [assert_no_orphans_in_xml_templates]    [xml_templates];
    [assert_no_orphans_in_xml_assets]       [xml_assets];
    [assert_no_orphans_in_xml_deletes]      [xml_deletes];
    [assert_no_orphans_in_js_files]         [js_files];
)]
impl SymbolTable {
    fn method_name(&self) {
        for (key, _) in self.field.iter() {
            if let Some(parent) = self.parent(key) {
                assert!(
                    self.is_key_valid(parent),
                    "{key:?} outlived its parent {parent:?}",
                );
            }
        }
    }
}

#[cfg(test)]
impl SymbolTable {
    /// Assert that no symbol outlived its parent, i.e. that a removal took its whole
    /// subtree with it.
    ///
    /// Reads each symbol's own parent field and never `children()`, so that a family
    /// missing from `register_parent_families!` cannot hide from this.
    pub(super) fn assert_no_orphans(&self) {
        self.assert_no_orphans_in_roots();
        self.assert_no_orphans_in_disk_dirs();
        self.assert_no_orphans_in_namespaces();
        self.assert_no_orphans_in_python_packages();
        self.assert_no_orphans_in_modules();
        self.assert_no_orphans_in_files();
        self.assert_no_orphans_in_compiled();
        self.assert_no_orphans_in_classes();
        self.assert_no_orphans_in_functions();
        self.assert_no_orphans_in_variables();
        self.assert_no_orphans_in_xml_files();
        self.assert_no_orphans_in_csv_files();
        self.assert_no_orphans_in_xml_records();
        self.assert_no_orphans_in_xml_fields();
        self.assert_no_orphans_in_xml_menuitems();
        self.assert_no_orphans_in_xml_templates();
        self.assert_no_orphans_in_xml_assets();
        self.assert_no_orphans_in_xml_deletes();
        self.assert_no_orphans_in_js_files();
    }
}
