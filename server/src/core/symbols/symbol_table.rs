use std::path::PathBuf;

use slotmap::{SlotMap, new_key_type};

use crate::{constants::{PackageType, SymType}, core::symbols::{
    class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol,
    disk_dir_symbol::DiskDirSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol,
    namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, root_symbol::RootSymbol,
    variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol,
}, threads::SessionInfo, utils::PathSanitizer};

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SymbolKey {
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

// AI-generated
macro_rules! impl_from_key {
    ($($variant:ident($key_type:ty)),* $(,)?) => {
        $(
            impl From<$key_type> for SymbolKey {
                fn from(key: $key_type) -> Self { SymbolKey::$variant(key) }
            }
        )*
    };
}

// Implements the From trait for each key type to allow easy conversion to SymbolKey
// enables key.into() to convert a specific key type into a SymbolKey
impl_from_key! {
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
pub enum SymbolView<'a> {
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

impl SymbolView<'_> {
    pub fn parent(&self) -> Option<SymbolKey> {
        match self {
            Self::Root(s) => s.parent,
            Self::DiskDir(s) => s.parent,
            Self::Namespace(s) => s.parent,
            Self::Package(s) => s.parent(),
            Self::File(s) => s.parent,
            Self::Compiled(s) => s.parent,
            Self::Class(s) => s.parent,
            Self::Function(s) => s.parent,
            Self::Variable(s) => s.parent,
            Self::XmlFileSymbol(s) => s.parent,
            Self::CsvFileSymbol(s) => s.parent,
        }
    }

    pub fn is_external(&self) -> bool {
        match self {
            Self::Root(_) => false,
            Self::DiskDir(d) => d.is_external,
            Self::Namespace(n) => n.is_external,
            Self::Package(p) => p.is_external(),
            Self::File(f) => f.is_external,
            Self::Compiled(c) => c.is_external,
            Self::Class(c) => c.is_external,
            Self::Function(f) => f.is_external,
            Self::Variable(v) => v.is_external,
            Self::XmlFileSymbol(x) => x.is_external,
            Self::CsvFileSymbol(c) => c.is_external,
        }
    }

    pub fn typ(&self) -> SymType {
        match self {
            Self::Root(_) => SymType::ROOT,
            Self::Namespace(_) => SymType::NAMESPACE,
            Self::DiskDir(_) => SymType::DISK_DIR,
            Self::Package(PackageSymbol::Module(_)) => SymType::PACKAGE(PackageType::MODULE),
            Self::Package(PackageSymbol::PythonPackage(_)) => SymType::PACKAGE(PackageType::PYTHON_PACKAGE),
            Self::File(_) => SymType::FILE,
            Self::Compiled(_) => SymType::COMPILED,
            Self::Class(_) => SymType::CLASS,
            Self::Function(_) => SymType::FUNCTION,
            Self::Variable(_) => SymType::VARIABLE,
            Self::XmlFileSymbol(_) => SymType::XML_FILE,
            Self::CsvFileSymbol(_) => SymType::CSV_FILE,
        }
    }

}

#[derive(Debug)]
pub struct SymbolTable {
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

    pub fn get_symbol(&self, key: SymbolKey) -> Option<SymbolView<'_>> {
        match key {
            SymbolKey::Root(k) => self.roots.get(k).map(SymbolView::Root),
            SymbolKey::DiskDir(k) => self.disk_dirs.get(k).map(SymbolView::DiskDir),
            SymbolKey::Namespace(k) => self.namespaces.get(k).map(SymbolView::Namespace),
            SymbolKey::Package(k) => self.packages.get(k).map(SymbolView::Package),
            SymbolKey::File(k) => self.files.get(k).map(SymbolView::File),
            SymbolKey::Compiled(k) => self.compiled.get(k).map(SymbolView::Compiled),
            SymbolKey::Class(k) => self.classes.get(k).map(SymbolView::Class),
            SymbolKey::Function(k) => self.functions.get(k).map(SymbolView::Function),
            SymbolKey::Variable(k) => self.variables.get(k).map(SymbolView::Variable),
            SymbolKey::XmlFile(k) => self.xml_files.get(k).map(SymbolView::XmlFileSymbol),
            SymbolKey::CsvFile(k) => self.csv_files.get(k).map(SymbolView::CsvFileSymbol),
        }
    }
    // pub fn get_symbol_mut(&mut self, key: SymbolKey) -> Option<SymbolMut<'_>> {
    //     match key {
    //         SymbolKey::Variable(k) => self.variables.get_mut(k).map(SymbolMut::Variable),
    //         SymbolKey::Function(k) => self.functions.get_mut(k).map(SymbolMut::Function),
    //     }
    // }


    // ========= former Symbol methods =========

    // Formerly called like self.find_module on a Symbol after borrowing the Rc/RefCell
    // No called directly with the key
    pub fn find_module(&self, key: SymbolKey) -> Option<SymbolKey> {
        let symbol = self.get_symbol(key)?;
        if let SymbolView::Package(PackageSymbol::Module(_)) = symbol {
            return Some(key);
        }
        return self.find_module(symbol.parent()?);
    }


    // ====== Symbol creation methods ======

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
    pub fn add_new_python_package(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let package_symbol = PackageSymbol::new_python_package(name, path, parent, is_external);
        let package_key = self.packages.insert(package_symbol);
        self.register_in_parent(parent, package_key.into(), name, path);
        package_key.into()
    }

    pub fn add_new_namespace(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.register_in_parent(parent, namespace_key.into(), name, path);
        namespace_key.into()
    }

    // @arena: not a method! (takes SessionInfo as arg)
    pub fn add_new_module_package(session: &mut SessionInfo, parent: SymbolKey, name: &String, path: &PathBuf) -> Option<SymbolKey> {
        let is_external = session.sync_odoo.symbol_table.parent_is_external(parent);
        let module = PackageSymbol::new_module_package(session, name.clone(), path, parent, is_external)?;
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

        
}
