use slotmap::{Key, new_key_type};

use crate::constants::{PackageType, SymType};

new_key_type! { pub struct RootKey; }
new_key_type! { pub struct DiskDirKey; }
new_key_type! { pub struct NamespaceKey; }
new_key_type! { pub struct PythonPackageKey; }
new_key_type! { pub struct ModuleKey; }
new_key_type! { pub struct FileKey; }
new_key_type! { pub struct CompiledKey; }
new_key_type! { pub struct ClassKey; }
new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct VariableKey; }
new_key_type! { pub struct XmlFileKey; }
new_key_type! { pub struct CsvFileKey; }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKey {
    Root(RootKey),
    DiskDir(DiskDirKey),
    Namespace(NamespaceKey),
    PythonPackage(PythonPackageKey),
    Module(ModuleKey),
    File(FileKey),
    Compiled(CompiledKey),
    Class(ClassKey),
    Function(FunctionKey),
    Variable(VariableKey),
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
}

impl SymbolKey {
    pub fn typ(&self) -> SymType {
        match self {
            Self::Root(_) => SymType::ROOT,
            Self::Namespace(_) => SymType::NAMESPACE,
            Self::DiskDir(_) => SymType::DISK_DIR,
            Self::Module(_) => SymType::PACKAGE(PackageType::MODULE),
            Self::PythonPackage(_) => SymType::PACKAGE(PackageType::PYTHON_PACKAGE),
            Self::File(_) => SymType::FILE,
            Self::Compiled(_) => SymType::COMPILED,
            Self::Class(_) => SymType::CLASS,
            Self::Function(_) => SymType::FUNCTION,
            Self::Variable(_) => SymType::VARIABLE,
            Self::XmlFile(_) => SymType::XML_FILE,
            Self::CsvFile(_) => SymType::CSV_FILE,
        }
    }

    pub fn unwrap_function_key(&self) -> FunctionKey {
        match self {
            SymbolKey::Function(k) => *k,
            _ => panic!("Not a FunctionKey"),
        }
    }

    pub fn unwrap_variable_key(&self) -> VariableKey {
        match self {
            SymbolKey::Variable(k) => *k,
            _ => panic!("Not a VariableKey"),
        }
    }

    pub fn unwrap_class_key(&self) -> ClassKey {
        match self {
            SymbolKey::Class(k) => *k,
            _ => panic!("Not a ClassKey"),
        }
    }

    pub fn unwrap_file_key(&self) -> FileKey {
        match self {
            SymbolKey::File(k) => *k,
            _ => panic!("Not a FileKey"),
        }
    }

    pub fn unwrap_python_package_key(&self) -> PythonPackageKey {
        match self {
            SymbolKey::PythonPackage(k) => *k,
            _ => panic!("Not a PythonPackageKey"),
        }
    }

    pub fn unwrap_module_key(&self) -> ModuleKey {
        match self {
            SymbolKey::Module(k) => *k,
            _ => panic!("Not a ModuleKey"),
        }
    }

    pub fn unwrap_namespace_key(&self) -> NamespaceKey {
        match self {
            SymbolKey::Namespace(k) => *k,
            _ => panic!("Not a NamespaceKey"),
        }
    }

    pub fn unwrap_root_key(&self) -> RootKey {
        match self {
            SymbolKey::Root(k) => *k,
            _ => panic!("Not a RootKey"),
        }
    }

    pub fn unwrap_xml_file_key(&self) -> XmlFileKey {
        match self {
            SymbolKey::XmlFile(k) => *k,
            _ => panic!("Not a XmlFileKey"),
        }
    }

    pub fn unwrap_csv_file_key(&self) -> CsvFileKey {
        match self {
            SymbolKey::CsvFile(k) => *k,
            _ => panic!("Not a CsvFileKey"),
        }
    }

}

pub trait KeyValidator<K> {
    fn is_key_valid(&self, key: K) -> bool;
}

/// Weak key. Wraps a key that might be invalid, and must be upgraded to be used.
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct Wk<K: Copy> {
    key: K,
}

impl<K: Copy> Wk<K> {
    pub fn upgrade(&self, table: &impl KeyValidator<K>) -> Option<K> {
        if table.is_key_valid(self.key) {
            Some(self.key)
        } else {
            None
        }
    }
    pub fn is_expired(&self, table: &impl KeyValidator<K>) -> bool {
        !table.is_key_valid(self.key)
    }

    /// Converts a Wk of a specific type to a Wk of a generic one, e.g.: Wk<FileKey> to Wk<SymbolKey>
    pub fn map_into<T: Copy>(self) -> Wk<T> where K: Into<T> {
        Wk { key: self.key.into() }
    }
}

impl Wk<SymbolKey> {
    pub fn null() -> Self {
        Self { key: RootKey::null().into() }
    }
}

/*
    Implements the From trait for each key type to allow easy conversion from
    specific key to generic SymbolKey
    E.g.:
    let f: FileKey = ...;

    // The traditional way to create a SymbolKey from a FileKey:
    let s = SymbolKey::File(f);

    // What the From trait allows:
    let s = SymbolKey::from(f);
    let s: SymbolKey = f.into();
 */
/// impl From<$key_type> for SymbolKey, e.g. From<FileKey> for SymbolKey
macro_rules! impl_from_key {
    ($($variant:ident($key_type:ty)),* $(,)?) => {
        $(
            impl From<$key_type> for SymbolKey {
                fn from(key: $key_type) -> Self { SymbolKey::$variant(key) }
            }
        )*
    };
}

impl_from_key! {
    Root(RootKey),
    DiskDir(DiskDirKey),
    Namespace(NamespaceKey),
    PythonPackage(PythonPackageKey),
    Module(ModuleKey),
    File(FileKey),
    Compiled(CompiledKey),
    Class(ClassKey),
    Function(FunctionKey),
    Variable(VariableKey),
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
}

// Converts to a Weak of the same key type, e.g. FileKey to Weak<FileKey>
impl<K: Copy> From<K> for Wk<K> {
    fn from(key: K) -> Self {
        Self { key }
    }
}

// Converts from a specific key type to a Weak of SymbolKey, e.g. FileKey to Weak<SymbolKey>
/// impl From<$key_type> for Weak<SymbolKey>, e.g. From<FileKey> for Weak<SymbolKey>
macro_rules! impl_weak_symbol_key_from {
    ($($key_type:ty),* $(,)?) => {
        $(
            impl From<$key_type> for Wk<SymbolKey> {
                fn from(key: $key_type) -> Self {
                    Self { key: key.into() }
                }
            }
        )*
    };
}

impl_weak_symbol_key_from! {
    RootKey,
    DiskDirKey,
    NamespaceKey,
    PythonPackageKey,
    ModuleKey,
    FileKey,
    CompiledKey,
    ClassKey,
    FunctionKey,
    VariableKey,
    XmlFileKey,
    CsvFileKey,
    SourceFileKey,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceFileKey {
    File(FileKey),
    PythonPackage(PythonPackageKey),
    Module(ModuleKey),
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
}

impl From<SourceFileKey> for SymbolKey {
    fn from(key: SourceFileKey) -> Self {
        match key {
            SourceFileKey::File(k) => k.into(),
            SourceFileKey::PythonPackage(k) => k.into(),
            SourceFileKey::Module(k) => k.into(),
            SourceFileKey::XmlFile(k) => k.into(),
            SourceFileKey::CsvFile(k) => k.into(),
        }
    }
}


impl From<FileKey> for SourceFileKey {
    fn from(key: FileKey) -> Self { SourceFileKey::File(key) }
}

impl From<PythonPackageKey> for SourceFileKey {
    fn from(key: PythonPackageKey) -> Self { SourceFileKey::PythonPackage(key) }
}

impl From<ModuleKey> for SourceFileKey {
    fn from(key: ModuleKey) -> Self { SourceFileKey::Module(key) }
}

impl From<XmlFileKey> for SourceFileKey {
    fn from(key: XmlFileKey) -> Self { SourceFileKey::XmlFile(key) }
}

impl From<CsvFileKey> for SourceFileKey {
    fn from(key: CsvFileKey) -> Self { SourceFileKey::CsvFile(key) }
}

impl SourceFileKey {
    pub fn unwrap_xml_file_key(&self) -> XmlFileKey {
        match self {
            SourceFileKey::XmlFile(k) => *k,
            _ => panic!("Not a XmlFileKey"),
        }
    }

    pub fn unwrap_file_key(&self) -> FileKey {
        match self {
            SourceFileKey::File(k) => *k,
            _ => panic!("Not a FileKey"),
        }
    }
}

impl SymbolKey {
    pub fn as_source_file_key(&self) -> Option<SourceFileKey> {
        match *self {
            SymbolKey::File(k) => Some(k.into()),
            SymbolKey::PythonPackage(k) => Some(k.into()),
            SymbolKey::Module(k) => Some(k.into()),
            SymbolKey::XmlFile(k) => Some(k.into()),
            SymbolKey::CsvFile(k) => Some(k.into()),
            _ => None,
        }
    }
}

/// Allows comparing a SymbolKey directly with its subtypes e.g.
/// symbol_key == file_key
/// symbol_key == source_file_key
macro_rules! impl_symbol_key_partial_eq {
    ($($key_type:ty),* $(,)?) => {
        $(
            impl PartialEq<$key_type> for SymbolKey {
                fn eq(&self, other: &$key_type) -> bool {
                    *self == SymbolKey::from(*other)
                }
            }
        )*
    };
}

impl_symbol_key_partial_eq! {
    RootKey, DiskDirKey, NamespaceKey, PythonPackageKey, ModuleKey,
    FileKey, CompiledKey, ClassKey, FunctionKey, VariableKey,
    XmlFileKey, CsvFileKey, SourceFileKey,
}
