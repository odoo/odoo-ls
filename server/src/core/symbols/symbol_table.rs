use slotmap::{SlotMap, new_key_type};

use crate::core::symbols::{
    class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol,
    disk_dir_symbol::DiskDirSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol,
    namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, root_symbol::RootSymbol,
    variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol,
};

new_key_type! { pub struct RootKey; }
new_key_type! { pub struct DiskDirKey; }
new_key_type! { pub struct NamespaceKey; }
new_key_type! { pub struct PackageKey; }
new_key_type! { pub struct FileKey; }
new_key_type! { pub struct CompiledKey; }
new_key_type! { pub struct ClassKey; }
new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct VariableKey; }
new_key_type! { pub struct XmlFileKey; }
new_key_type! { pub struct CsvFileKey; }

#[derive(Clone, Copy)]
enum SymbolKey {
    Root(RootKey),
    DiskDir(DiskDirKey),
    Namespace(NamespaceKey),
    Package(PackageKey),
    File(FileKey),
    Compiled(CompiledKey),
    Class(ClassKey),
    Function(FunctionKey),
    Variable(VariableKey),
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
}

#[derive(Debug)]
pub enum SymbolNew<'a> {
    Root(&'a RootSymbol),
    DiskDir(&'a DiskDirSymbol),
    Namespace(&'a NamespaceSymbol),
    Package(&'a PackageSymbol),
    File(&'a FileSymbol),
    Compiled(&'a CompiledSymbol),
    Class(&'a ClassSymbol),
    Function(&'a FunctionSymbol),
    Variable(&'a VariableSymbol),
    XmlFileSymbol(&'a XmlFileSymbol),
    CsvFileSymbol(&'a CsvFileSymbol),
}

struct SymbolTable {
    pub roots: SlotMap<RootKey, RootSymbol>,
    pub disk_dirs: SlotMap<DiskDirKey, DiskDirSymbol>,
    pub namespaces: SlotMap<NamespaceKey, NamespaceSymbol>,
    pub packages: SlotMap<PackageKey, PackageSymbol>,
    pub files: SlotMap<FileKey, FileSymbol>,
    pub compiled: SlotMap<CompiledKey, CompiledSymbol>,
    pub classes: SlotMap<ClassKey, ClassSymbol>,
    pub functions: SlotMap<FunctionKey, FunctionSymbol>,
    pub variables: SlotMap<VariableKey, VariableSymbol>,
    pub xml_files: SlotMap<XmlFileKey, XmlFileSymbol>,
    pub csv_files: SlotMap<CsvFileKey, CsvFileSymbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            roots: SlotMap::with_key(),
            disk_dirs: SlotMap::with_key(),
            namespaces: SlotMap::with_key(),
            packages: SlotMap::with_key(),
            files: SlotMap::with_key(),
            compiled: SlotMap::with_key(),
            classes: SlotMap::with_key(),
            functions: SlotMap::with_key(),
            variables: SlotMap::with_key(),
            xml_files: SlotMap::with_key(),
            csv_files: SlotMap::with_key(),
        }
    }

    pub fn insert_variable(&mut self, symbol: VariableSymbol) -> SymbolKey {
        let key = self.variables.insert(symbol);
        SymbolKey::Variable(key)
    }

    pub fn insert_function(&mut self, symbol: FunctionSymbol) -> SymbolKey {
        let key = self.functions.insert(symbol);
        SymbolKey::Function(key)
    }

    pub fn get_symbol(&self, key: SymbolKey) -> Option<SymbolNew<'_>> {
        match key {
            SymbolKey::Root(k) => self.roots.get(k).map(SymbolNew::Root),
            SymbolKey::DiskDir(k) => self.disk_dirs.get(k).map(SymbolNew::DiskDir),
            SymbolKey::Namespace(k) => self.namespaces.get(k).map(SymbolNew::Namespace),
            SymbolKey::Package(k) => self.packages.get(k).map(SymbolNew::Package),
            SymbolKey::File(k) => self.files.get(k).map(SymbolNew::File),
            SymbolKey::Compiled(k) => self.compiled.get(k).map(SymbolNew::Compiled),
            SymbolKey::Class(k) => self.classes.get(k).map(SymbolNew::Class),
            SymbolKey::Function(k) => self.functions.get(k).map(SymbolNew::Function),
            SymbolKey::Variable(k) => self.variables.get(k).map(SymbolNew::Variable),
            SymbolKey::XmlFile(k) => self.xml_files.get(k).map(SymbolNew::XmlFileSymbol),
            SymbolKey::CsvFile(k) => self.csv_files.get(k).map(SymbolNew::CsvFileSymbol),
        }
    }
    // pub fn get_symbol_mut(&mut self, key: SymbolKey) -> Option<SymbolMut<'_>> {
    //     match key {
    //         SymbolKey::Variable(k) => self.variables.get_mut(k).map(SymbolMut::Variable),
    //         SymbolKey::Function(k) => self.functions.get_mut(k).map(SymbolMut::Function),
    //     }
    // }
}
