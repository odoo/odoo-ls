use duplicate::duplicate_item;
use odoo_ls_macros::{From, IntoKey, IntoSymbolKey, SymbolKeySubset, Validator};
use slotmap::{Key, new_key_type};

use crate::constants::{OYarn, PackageType, SymType};

new_key_type! {
    pub struct RootKey;
    pub struct DiskDirKey;
    pub struct NamespaceKey;
    pub struct PythonPackageKey;
    pub struct ModuleKey;
    pub struct FileKey;
    pub struct CompiledKey;
    pub struct ClassKey;
    pub struct FunctionKey;
    pub struct VariableKey;
    pub struct XmlFileKey;
    pub struct XmlRecordKey;
    pub struct XmlFieldKey;
    pub struct XmlMenuItemKey;
    pub struct XmlTemplateKey;
    pub struct XmlAssetKey;
    pub struct XmlDeleteKey;
    pub struct CsvFileKey;
}

new_key_type! { pub struct JsFileKey; }

/// A class symbol paired with its origin module's `dir_name`, set only when the
/// class lives outside the queried module's dependencies (`None` when in-deps).
pub type ClassWithModule = (ClassKey, Option<OYarn>);
/// A member symbol paired with its origin module's `dir_name` (see [`ClassWithModule`]).
pub type MemberWithModule = (SymbolKey, Option<OYarn>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, From, Validator)]
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
    XmlRecord(XmlRecordKey),
    XmlField(XmlFieldKey),
    XmlMenuItem(XmlMenuItemKey),
    XmlTemplate(XmlTemplateKey),
    XmlAsset(XmlAssetKey),
    XmlDelete(XmlDeleteKey),
    CsvFile(CsvFileKey),
    JsFile(JsFileKey),
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
            Self::XmlRecord(_) => SymType::XML_RECORD,
            Self::XmlField(_) => SymType::XML_FIELD,
            Self::XmlMenuItem(_) => SymType::XML_MENUITEM,
            Self::XmlTemplate(_) => SymType::XML_TEMPLATE,
            Self::XmlAsset(_) => SymType::XML_ASSET,
            Self::XmlDelete(_) => SymType::XML_DELETE,
            Self::CsvFile(_) => SymType::CSV_FILE,
            Self::JsFile(_) => SymType::JS_FILE,
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

    pub fn unwrap_xml_record_key(&self) -> XmlRecordKey {
        match self {
            SymbolKey::XmlRecord(k) => *k,
            _ => panic!("Not a XmlRecordKey"),
        }
    }

    pub fn unwrap_xml_field_key(&self) -> XmlFieldKey {
        match self {
            SymbolKey::XmlField(k) => *k,
            _ => panic!("Not a XmlFieldKey"),
        }
    }

    pub fn unwrap_xml_menu_item_key(&self) -> XmlMenuItemKey {
        match self {
            SymbolKey::XmlMenuItem(k) => *k,
            _ => panic!("Not a XmlMenuItemKey"),
        }
    }

    pub fn unwrap_xml_template_key(&self) -> XmlTemplateKey {
        match self {
            SymbolKey::XmlTemplate(k) => *k,
            _ => panic!("Not a XmlTemplateKey"),
        }
    }

    pub fn unwrap_xml_asset_key(&self) -> XmlAssetKey {
        match self {
            SymbolKey::XmlAsset(k) => *k,
            _ => panic!("Not a XmlAssetKey"),
        }
    }

    pub fn unwrap_xml_delete_key(&self) -> XmlDeleteKey {
        match self {
            SymbolKey::XmlDelete(k) => *k,
            _ => panic!("Not a XmlDeleteKey"),
        }
    }

    pub fn unwrap_csv_file_key(&self) -> CsvFileKey {
        match self {
            SymbolKey::CsvFile(k) => *k,
            _ => panic!("Not a CsvFileKey"),
        }
    }

    pub fn unwrap_js_file_key(&self) -> JsFileKey {
        match self {
            SymbolKey::JsFile(k) => *k,
            _ => panic!("Not a JsFileKey"),
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

// Converts to a Weak of the same key type, e.g. FileKey to Weak<FileKey>
impl<K: Copy> From<K> for Wk<K> {
    fn from(key: K) -> Self {
        Self { key }
    }
}

// Converts from a specific key type to a Weak of SymbolKey, e.g. FileKey to Weak<SymbolKey>
/// impl From<$key_type> for Weak<SymbolKey>, e.g. From<FileKey> for Weak<SymbolKey>
#[duplicate_item(
    key_type;
    [RootKey];
    [DiskDirKey];
    [NamespaceKey];
    [PythonPackageKey];
    [ModuleKey];
    [FileKey];
    [CompiledKey];
    [ClassKey];
    [FunctionKey];
    [VariableKey];
    [XmlFileKey];
    [XmlRecordKey];
    [XmlFieldKey];
    [XmlMenuItemKey];
    [XmlTemplateKey];
    [XmlAssetKey];
    [XmlDeleteKey];
    [CsvFileKey];
    [JsFileKey];
    [SourceFileKey];
    [ModelSymbolKey];
)]
impl From<key_type> for Wk<SymbolKey> {
    fn from(key: key_type) -> Self {
        Self { key: key.into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SymbolKeySubset, IntoKey)]
#[into_key(XmlId)]
pub enum XmlDataKey {
    XmlRecord(XmlRecordKey),
    XmlMenuItem(XmlMenuItemKey),
    XmlTemplate(XmlTemplateKey),
    XmlAsset(XmlAssetKey),
    XmlDelete(XmlDeleteKey),
}

impl XmlDataKey {
    pub fn as_xml_record_key(&self) -> Option<XmlRecordKey> {
        match self {
            XmlDataKey::XmlRecord(k) => Some(*k),
            _ => None,
        }
    }

    pub fn as_xml_template_key(&self) -> Option<XmlTemplateKey> {
        match self {
            XmlDataKey::XmlTemplate(k) => Some(*k),
            _ => None,
        }
    }
}

impl SymbolKey {
    pub fn as_xml_data_key(&self) -> Option<XmlDataKey> {
        XmlDataKey::try_from(*self).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SymbolKeySubset, IntoKey)]
#[into_key(BuildableSymbolKey)]
pub enum SourceFileKey {
    File(FileKey),
    PythonPackage(PythonPackageKey),
    Module(ModuleKey),
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
    JsFile(JsFileKey),
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

    pub fn unwrap_csv_file_key(&self) -> CsvFileKey {
        match self {
            SourceFileKey::CsvFile(k) => *k,
            _ => panic!("Not a CsvFileKey"),
        }
    }
}

impl SymbolKey {
    pub fn as_source_file_key(&self) -> Option<SourceFileKey> {
        SourceFileKey::try_from(*self).ok()
    }
}

/// Allows comparing a SymbolKey directly with its subtypes e.g.
/// symbol_key == file_key
/// symbol_key == source_file_key
#[duplicate_item(
    key_type;
    [RootKey];
    [DiskDirKey];
    [NamespaceKey];
    [PythonPackageKey];
    [ModuleKey];
    [FileKey];
    [CompiledKey];
    [ClassKey];
    [FunctionKey];
    [VariableKey];
    [XmlFileKey];
    [XmlRecordKey];
    [XmlFieldKey];
    [XmlMenuItemKey];
    [XmlTemplateKey];
    [XmlAssetKey];
    [XmlDeleteKey];
    [CsvFileKey];
    [JsFileKey];
    [SourceFileKey];
)]
impl PartialEq<key_type> for SymbolKey {
    fn eq(&self, other: &key_type) -> bool {
        *self == SymbolKey::from(*other)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, From, IntoSymbolKey, Validator)]
pub enum XmlId {
    PythonClass(ClassKey),
    XmlRecord(XmlRecordKey),
    XmlMenuItem(XmlMenuItemKey),
    XmlTemplate(XmlTemplateKey),
    XmlAsset(XmlAssetKey),
    XmlDelete(XmlDeleteKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, From, IntoSymbolKey, Validator)]
pub enum ModelSymbolKey {
    // [Partial]Ord is implemented to sort ClassKeys before XmlRecordKeys
    Class(ClassKey),
    XmlRecord(XmlRecordKey),
}

impl ModelSymbolKey {
    pub fn as_class_key(&self) -> Option<ClassKey> {
        match self {
            ModelSymbolKey::Class(k) => Some(*k),
            _ => None,
        }
    }

    pub fn as_xml_record_key(&self) -> Option<XmlRecordKey> {
        match self {
            ModelSymbolKey::XmlRecord(k) => Some(*k),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SymbolKeySubset)]
pub enum JsFileParent {
    Module(ModuleKey),
    DiskDir(DiskDirKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SymbolKeySubset)]
pub enum BuildableSymbolKey {
    Function(FunctionKey),
    File(FileKey),
    Module(ModuleKey),
    PythonPackage(PythonPackageKey),
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
    JsFile(JsFileKey),
}

impl BuildableSymbolKey {
    pub fn as_source_file_key(&self) -> Option<SourceFileKey> {
        match *self {
            BuildableSymbolKey::File(k) => Some(k.into()),
            BuildableSymbolKey::PythonPackage(k) => Some(k.into()),
            BuildableSymbolKey::Module(k) => Some(k.into()),
            BuildableSymbolKey::XmlFile(k) => Some(k.into()),
            BuildableSymbolKey::CsvFile(k) => Some(k.into()),
            BuildableSymbolKey::JsFile(k) => Some(k.into()),
            _ => None,
        }
    }
}

impl SymbolKey {
    pub fn as_buildable_symbol_key(&self) -> Option<BuildableSymbolKey> {
        match *self {
            SymbolKey::Function(k) => Some(BuildableSymbolKey::Function(k)),
            SymbolKey::File(k) => Some(BuildableSymbolKey::File(k)),
            SymbolKey::Module(k) => Some(BuildableSymbolKey::Module(k)),
            SymbolKey::PythonPackage(k) => Some(BuildableSymbolKey::PythonPackage(k)),
            SymbolKey::XmlFile(k) => Some(BuildableSymbolKey::XmlFile(k)),
            SymbolKey::CsvFile(k) => Some(BuildableSymbolKey::CsvFile(k)),
            SymbolKey::JsFile(k) => Some(BuildableSymbolKey::JsFile(k)),
            _ => None,
        }
    }
    pub fn unwrap_buildable_key(&self) -> BuildableSymbolKey {
        match *self {
            SymbolKey::Function(k) => BuildableSymbolKey::Function(k),
            SymbolKey::File(k) => BuildableSymbolKey::File(k),
            SymbolKey::Module(k) => BuildableSymbolKey::Module(k),
            SymbolKey::PythonPackage(k) => BuildableSymbolKey::PythonPackage(k),
            SymbolKey::XmlFile(k) => BuildableSymbolKey::XmlFile(k),
            SymbolKey::CsvFile(k) => BuildableSymbolKey::CsvFile(k),
            SymbolKey::JsFile(k) => BuildableSymbolKey::JsFile(k),
            _ => panic!("Not a buildable symbol key"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SymbolKeySubset, IntoKey)]
#[into_key(BuildableSymbolKey)]
pub enum PythonBuildableSymbolKey {
    Function(FunctionKey),
    File(FileKey),
    Module(ModuleKey),
    PythonPackage(PythonPackageKey),
}

impl BuildableSymbolKey {
    pub fn as_python_buildable(&self) -> Option<PythonBuildableSymbolKey> {
        match *self {
            BuildableSymbolKey::File(k) => Some(k.into()),
            BuildableSymbolKey::Function(k) => Some(k.into()),
            BuildableSymbolKey::PythonPackage(k) => Some(k.into()),
            BuildableSymbolKey::Module(k) => Some(k.into()),
            _ => None,
        }
    }
}
impl SymbolKey {
    pub fn as_python_buildable(&self) -> Option<PythonBuildableSymbolKey> {
        match *self {
            SymbolKey::Function(k) => Some(PythonBuildableSymbolKey::Function(k)),
            SymbolKey::File(k) => Some(PythonBuildableSymbolKey::File(k)),
            SymbolKey::Module(k) => Some(PythonBuildableSymbolKey::Module(k)),
            SymbolKey::PythonPackage(k) => Some(PythonBuildableSymbolKey::PythonPackage(k)),
            _ => None,
        }
    }
}
