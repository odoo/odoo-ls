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
        ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, JsFileKey, KeyValidator, ModelSymbolKey, ModuleKey, NamespaceKey, PythonPackageKey, RootKey, SourceFileKey, SymbolKey, VariableKey, XmlAssetKey, XmlDataKey, XmlDeleteKey, XmlFieldKey, XmlFileKey, XmlId, XmlMenuItemKey, XmlRecordKey, XmlTemplateKey
    }
}};
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
    Implement Index and IndexMut for each symbol type.
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

macro_rules! impl_index {
    ($key:ty, $output:ty, $field:ident) => {
        impl Index<$key> for SymbolTable {
            type Output = $output;
            fn index(&self, key: $key) -> &$output {
                &self.$field[key]
            }
        }
        impl IndexMut<$key> for SymbolTable {
            fn index_mut(&mut self, key: $key) -> &mut $output {
                &mut self.$field[key]
            }
        }
    };
}

impl_index!(RootKey, RootSymbol, roots);
impl_index!(DiskDirKey, DiskDirSymbol, disk_dirs);
impl_index!(NamespaceKey, NamespaceSymbol, namespaces);
impl_index!(PythonPackageKey, PythonPackageSymbol, python_packages);
impl_index!(ModuleKey, ModuleSymbol, modules);
impl_index!(FileKey, FileSymbol, files);
impl_index!(CompiledKey, CompiledSymbol, compiled);
impl_index!(FunctionKey, FunctionSymbol, functions);
impl_index!(ClassKey, ClassSymbol, classes);
impl_index!(VariableKey, VariableSymbol, variables);
impl_index!(XmlFileKey, XmlFileSymbol, xml_files);
impl_index!(XmlRecordKey, XmlRecordSymbol, xml_records);
impl_index!(XmlFieldKey, XmlFieldSymbol, xml_fields);
impl_index!(XmlMenuItemKey, XmlMenuItemSymbol, xml_menuitems);
impl_index!(XmlTemplateKey, XmlTemplateSymbol, xml_templates);
impl_index!(XmlAssetKey, XmlAssetSymbol, xml_assets);
impl_index!(XmlDeleteKey, XmlDeleteSymbol, xml_deletes);
impl_index!(CsvFileKey, CsvFileSymbol, csv_files);
impl_index!(JsFileKey, JsFileSymbol, js_files);


/*
    Implement KeyValidator for each symbol type, to check if a key is valid.
    This allows us to pass the symbol table as argument to methods that expect a
    impl KeyValidator, like upgrade(), e.g.:

    let symbol_table: SymbolTable = ...;
    let weak_key: Weak<SymbolKey> = ...;

    let maybe_valid_key: Option<SymbolKey> = weak_key.upgrade(&symbol_table);

    let weak_file_key: Weak<FileKey> = ...;

    let maybe_valid_file_key: Option<FileKey> = weak_file_key.upgrade(&symbol_table);
 */

macro_rules! impl_key_validator {
    ($key:ty, $field:ident) => {
        impl KeyValidator<$key> for SymbolTable {
            fn is_key_valid(&self, key: $key) -> bool {
                self.$field.contains_key(key)
            }
        }
    };
}

impl_key_validator!(RootKey, roots);
impl_key_validator!(DiskDirKey, disk_dirs);
impl_key_validator!(NamespaceKey, namespaces);
impl_key_validator!(PythonPackageKey, python_packages);
impl_key_validator!(ModuleKey, modules);
impl_key_validator!(FileKey, files);
impl_key_validator!(CompiledKey, compiled);
impl_key_validator!(ClassKey, classes);
impl_key_validator!(FunctionKey, functions);
impl_key_validator!(VariableKey, variables);
impl_key_validator!(XmlFileKey, xml_files);
impl_key_validator!(XmlRecordKey, xml_records);
impl_key_validator!(XmlFieldKey, xml_fields);
impl_key_validator!(XmlMenuItemKey, xml_menuitems);
impl_key_validator!(XmlTemplateKey, xml_templates);
impl_key_validator!(XmlAssetKey, xml_assets);
impl_key_validator!(XmlDeleteKey, xml_deletes);
impl_key_validator!(CsvFileKey, csv_files);
impl_key_validator!(JsFileKey, js_files);

impl KeyValidator<SymbolKey> for SymbolTable {
    fn is_key_valid(&self, key: SymbolKey) -> bool {
        match key {
            SymbolKey::Root(k) => self.roots.contains_key(k),
            SymbolKey::DiskDir(k) => self.disk_dirs.contains_key(k),
            SymbolKey::Namespace(k) => self.namespaces.contains_key(k),
            SymbolKey::PythonPackage(k) => self.python_packages.contains_key(k),
            SymbolKey::Module(k) => self.modules.contains_key(k),
            SymbolKey::File(k) => self.files.contains_key(k),
            SymbolKey::Compiled(k) => self.compiled.contains_key(k),
            SymbolKey::Class(k) => self.classes.contains_key(k),
            SymbolKey::Function(k) => self.functions.contains_key(k),
            SymbolKey::Variable(k) => self.variables.contains_key(k),
            SymbolKey::XmlFile(k) => self.xml_files.contains_key(k),
            SymbolKey::XmlRecord(k) => self.xml_records.contains_key(k),
            SymbolKey::XmlField(k) => self.xml_fields.contains_key(k),
            SymbolKey::XmlMenuItem(k) => self.xml_menuitems.contains_key(k),
            SymbolKey::XmlTemplate(k) => self.xml_templates.contains_key(k),
            SymbolKey::XmlAsset(k) => self.xml_assets.contains_key(k),
            SymbolKey::XmlDelete(k) => self.xml_deletes.contains_key(k),
            SymbolKey::CsvFile(k) => self.csv_files.contains_key(k),
            SymbolKey::JsFile(k) => self.js_files.contains_key(k),
        }
    }
}

impl KeyValidator<SourceFileKey> for SymbolTable {
    fn is_key_valid(&self, key: SourceFileKey) -> bool {
        match key {
            SourceFileKey::File(k) => self.files.contains_key(k),
            SourceFileKey::PythonPackage(k) => self.python_packages.contains_key(k),
            SourceFileKey::Module(k) => self.modules.contains_key(k),
            SourceFileKey::XmlFile(k) => self.xml_files.contains_key(k),
            SourceFileKey::CsvFile(k) => self.csv_files.contains_key(k),
            SourceFileKey::JsFile(k) => self.js_files.contains_key(k),
        }
    }
}

impl KeyValidator<XmlDataKey> for SymbolTable {
    fn is_key_valid(&self, key: XmlDataKey) -> bool {
        match key {
            XmlDataKey::XmlRecord(k) => self.xml_records.contains_key(k),
            XmlDataKey::XmlMenuItem(k) => self.xml_menuitems.contains_key(k),
            XmlDataKey::XmlTemplate(k) => self.xml_templates.contains_key(k),
            XmlDataKey::XmlAsset(k) => self.xml_assets.contains_key(k),
            XmlDataKey::XmlDelete(k) => self.xml_deletes.contains_key(k),
        }
    }
}

impl KeyValidator<XmlId> for SymbolTable {
    fn is_key_valid(&self, key: XmlId) -> bool {
        match key {
            XmlId::PythonClass(k) => self.classes.contains_key(k),
            XmlId::XmlRecord(k) => self.xml_records.contains_key(k),
            XmlId::XmlMenuItem(k) => self.xml_menuitems.contains_key(k),
            XmlId::XmlTemplate(k) => self.xml_templates.contains_key(k),
            XmlId::XmlAsset(k) => self.xml_assets.contains_key(k),
            XmlId::XmlDelete(k) => self.xml_deletes.contains_key(k),
        }
    }
}

impl KeyValidator<ModelSymbolKey> for SymbolTable {
    fn is_key_valid(&self, key: ModelSymbolKey) -> bool {
        match key {
            ModelSymbolKey::Class(k) => self.classes.contains_key(k),
            ModelSymbolKey::XmlRecord(k) => self.xml_records.contains_key(k),
        }
    }
}
