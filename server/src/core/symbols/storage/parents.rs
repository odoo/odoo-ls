use odoo_ls_macros::{SymbolKeySubset};
use crate::{
    constants::OYarn,
    core::symbols::{
        symbol_keys::{
            ClassKey, CompiledKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, JsFileKey,
            ModuleKey, NamespaceKey, PythonPackageKey, RootKey, SymbolKey,
            XmlAssetKey, XmlDataKey, XmlFieldKey, XmlFileKey, XmlRecordKey,
        },
        SymbolMgr,
    },
    oyarn,
    utils::{HashMap, HashSet},
};
use super::SymbolTable;

// ==== File content ====

#[derive(Debug, Clone, Copy, SymbolKeySubset)]
pub enum FileContentParent {
    File(FileKey),
    PythonPackage(PythonPackageKey),
    Module(ModuleKey),
    Class(ClassKey),
    Function(FunctionKey),
}

impl FileContentParent {
    pub fn as_symbol_mgr(self, st: &SymbolTable) -> &dyn SymbolMgr {
        match self {
            Self::File(f) => &st[f],
            Self::PythonPackage(p) => &st[p],
            Self::Module(m) => &st[m],
            Self::Class(c) => &st[c],
            Self::Function(f) => &st[f],
        }
    }
    
    pub(super) fn symbols_mut(self, st: &mut SymbolTable) -> &mut HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>> {
        match self {
            Self::File(f) => &mut st[f].symbols,
            Self::PythonPackage(p) => &mut st[p].symbols,
            Self::Module(m) => &mut st[m].symbols,
            Self::Class(c) => &mut st[c].symbols,
            Self::Function(f) => &mut st[f].symbols,
        }
    }
    
    pub(super) fn add_child(self, st: &mut SymbolTable, name: &str, child: SymbolKey, position: u32) {
        let section = self.as_symbol_mgr(st).get_section_for(position).index;
        let name = oyarn!("{}", name);
        self.symbols_mut(st).entry(name).or_default()
            .entry(section).or_default()
            .push(child);
    } 
    
    pub fn children(&self, st: &SymbolTable) -> impl Iterator<Item = SymbolKey> {
        self.as_symbol_mgr(st).symbols().values()
            .flat_map(|section| section.values())
            .flatten()
            .copied()
    }
}

// ==== Filesystem items (aka module symbols) ====

#[derive(Debug, Clone, Copy, SymbolKeySubset)]
pub enum FileSystemSymbolParent {
    Root(RootKey),
    DiskDir(DiskDirKey),
    Namespace(NamespaceKey),
    PythonPackage(PythonPackageKey),
    Module(ModuleKey),
    Compiled(CompiledKey),
}

impl FileSystemSymbolParent {
    /// Helper for Self::children and get_child
    fn fs_symbols(self, st: &SymbolTable) -> &HashMap<OYarn, SymbolKey> {
        match self {
            Self::Root(r) => &st[r].fs_symbols,
            Self::DiskDir(d) => &st[d].fs_symbols,
            Self::PythonPackage(p) => &st[p].fs_symbols,
            Self::Module(m) => &st[m].fs_symbols,
            Self::Compiled(c) => &st[c].fs_symbols,
            Self::Namespace(_) => panic!("caller handles namespace separately")
        }
    }
    
    /// Helper for add/remove_fs_symbol
    fn fs_symbols_mut(self, st: &mut SymbolTable) -> &mut HashMap<OYarn, SymbolKey> {
        match self {
            Self::Root(r) => &mut st[r].fs_symbols,
            Self::DiskDir(d) => &mut st[d].fs_symbols,
            Self::PythonPackage(p) => &mut st[p].fs_symbols,
            Self::Module(m) => &mut st[m].fs_symbols,
            Self::Compiled(c) => &mut st[c].fs_symbols,
            Self::Namespace(_) => panic!("caller handles namespace separately")
        }
    }
    
    pub(super) fn add_fs_symbol(self, st: &mut SymbolTable, name: &str, child: SymbolKey, path: &str) -> Option<SymbolKey> {
        if let Self::Namespace(ns) = self {
            return st[ns].add_child(name, child, path);
        }
        self.fs_symbols_mut(st).insert(oyarn!("{}", name), child)
    }

    pub(super) fn remove_fs_symbol(self, st: &mut SymbolTable, name: &str) {
        if let Self::Namespace(ns) = self {
            return st[ns].remove_child(name);
        }
        self.fs_symbols_mut(st).remove(name);
    }
    
    pub fn get_child(self, st: &SymbolTable, name: &str) -> Option<SymbolKey> {
        if let Self::Namespace(ns) = self {
            return st[ns].get_child(name);
        }
        self.fs_symbols(st).get(name).copied()
    }
    
    pub fn children(self, st: &SymbolTable) -> Vec<SymbolKey> {
        if let Self::Namespace(ns) = self {
            return st[ns].children().collect();
        }
        self.fs_symbols(st).values().copied().collect()
    }
}

// ==== Js symbols ====

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SymbolKeySubset)]
pub enum JsFileParent {
    Module(ModuleKey),
    DiskDir(DiskDirKey),
}

impl JsFileParent {
    pub fn js_symbols(self, st: &SymbolTable) -> &HashMap<String, JsFileKey> {
        match self {
            Self::Module(m) => &st[m].js_symbols,
            Self::DiskDir(d) => &st[d].js_symbols,
        }
    }
    
    pub(super) fn js_symbols_mut(self, st: &mut SymbolTable) -> &mut HashMap<String, JsFileKey> {
        match self {
            Self::Module(m) => &mut st[m].js_symbols,
            Self::DiskDir(d) => &mut st[d].js_symbols,
        }
    }
}

// ==== Xml data ====

#[derive(Debug, Clone, Copy, SymbolKeySubset)]
pub enum XmlDataParent {
    XmlFile(XmlFileKey),
    CsvFile(CsvFileKey),
}

impl XmlDataParent {
    pub fn data_symbols(self, st: &SymbolTable) -> &HashSet<XmlDataKey> {
        match self {
            Self::XmlFile(x) => &st[x].data_symbols,
            Self::CsvFile(c) => &st[c].data_symbols,
        }
    }
    
    pub(super) fn data_symbols_mut(self, st: &mut SymbolTable) -> &mut HashSet<XmlDataKey> {
        match self {
            Self::XmlFile(x) => &mut st[x].data_symbols,
            Self::CsvFile(c) => &mut st[c].data_symbols,
        }
    }
}

// ==== Xml Fields ====

#[derive(Debug, Clone, Copy, SymbolKeySubset)]
pub enum XmlFieldParent {
    XmlRecord(XmlRecordKey),
    XmlAsset(XmlAssetKey),
}

impl XmlFieldParent {
    pub fn fields(self, st: &SymbolTable) -> &HashMap<OYarn, XmlFieldKey> {
        match self {
            Self::XmlRecord(x) => &st[x].fields,
            Self::XmlAsset(x) => &st[x].fields,
        }
    }
    
    pub fn fields_mut(self, st: &mut SymbolTable) -> &mut HashMap<OYarn, XmlFieldKey> {
        match self {
            Self::XmlRecord(x) => &mut st[x].fields,
            Self::XmlAsset(x) => &mut st[x].fields,
        }
    }
}
