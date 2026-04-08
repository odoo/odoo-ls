use slotmap::{Key, new_key_type};

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

}

pub trait ContainsKey<K> {
    fn contains_key(&self, key: K) -> bool;
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub struct Weak<K: Copy> {
    key: K,
}

impl<K: Copy> Weak<K> {
    pub fn upgrade(&self, table: &impl ContainsKey<K>) -> Option<K> {
        if table.contains_key(self.key) {
            Some(self.key)
        } else {
            None
        }
    }
    pub fn is_expired(&self, table: &impl ContainsKey<K>) -> bool {
        !table.contains_key(self.key)
    }
}
    
impl Weak<SymbolKey> {
    pub fn null() -> Self {
        Self { key: RootKey::null().into() }
    }
}

impl<K: Copy> From<K> for Weak<K> {
    fn from(key: K) -> Self {
        Self { key }
    }
}

impl From<ClassKey> for Weak<SymbolKey> {
    fn from(key: ClassKey) -> Self {
        Self { key: SymbolKey::Class(key) }
    }
}

impl From<FunctionKey> for Weak<SymbolKey> {
    fn from(key: FunctionKey) -> Self {
        Self { key: SymbolKey::Function(key) }
    }
}

impl From<CsvFileKey> for Weak<SymbolKey> {
    fn from(key: CsvFileKey) -> Self {
        Self { key: SymbolKey::CsvFile(key) }
    }
}

impl From<XmlFileKey> for Weak<SymbolKey> {
    fn from(key: XmlFileKey) -> Self {
        Self { key: SymbolKey::XmlFile(key) }
    }
}

impl From<ModuleKey> for Weak<SymbolKey> {
    fn from(key: ModuleKey) -> Self {
        Self { key: SymbolKey::Module(key) }
    }
}
