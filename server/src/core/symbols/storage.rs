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
mod ext_symbol_store;

use crate::{constants::OYarn, core::symbols::{
    ClassSymbol, CompiledSymbol, CsvFileSymbol, DiskDirSymbol, FileSymbol, FunctionSymbol, JsFileSymbol, ModuleSymbol, NamespaceSymbol, PythonPackageSymbol, RootSymbol, VariableSymbol, XmlFileSymbol, storage::xml::{xml_asset_symbol::XmlAssetSymbol, xml_delete_symbol::XmlDeleteSymbol, xml_field_symbol::XmlFieldSymbol, xml_menuitem_symbol::XmlMenuItemSymbol, xml_record_symbol::XmlRecordSymbol, xml_template_symbol::XmlTemplateSymbol}, symbol_keys::{
        ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, JsFileKey, KeyValidator, ModuleKey, NamespaceKey, PythonPackageKey, RootKey, VariableKey, XmlAssetKey, XmlDeleteKey, XmlFieldKey, XmlFileKey, XmlMenuItemKey, XmlRecordKey, XmlTemplateKey
    }
}};
use duplicate::duplicate;
use ext_symbol_store::ExtSymbolStore;
use slotmap::{SlotMap, SparseSecondaryMap};
use std::ops::{Index, IndexMut};

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
