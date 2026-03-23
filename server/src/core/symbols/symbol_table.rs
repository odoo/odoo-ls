use std::{cell::RefCell, cmp::Ordering, collections::{HashMap, HashSet, VecDeque, hash_map}, path::PathBuf, rc::Rc};

use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::ExprCall;
use ruff_text_size::TextRange;
use slotmap::{Key, SlotMap, new_key_type};
use tracing::trace;

use crate::{S, Sy, constants::{BuildStatus, BuildSteps, OYarn, PackageType, SymType, Tree, flatten_tree}, core::{diagnostics::{DiagnosticCode,
create_diagnostic}, entry_point::EntryPoint, evaluation::{Context, ContextValue,
Evaluation, EvaluationSymbolPtr}, file_mgr::NoqaInfo, model::Model, odoo::SyncOdoo, python_validator::PythonValidator, symbols::{ class_symbol::ClassSymbol,
compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol,
dependency_mgr::{Buildable, Dependencies}, disk_dir_symbol::DiskDirSymbol,
ext_symbol_store::ExtSymbolStore, file_symbol::FileSymbol,
function_symbol::{Argument, FunctionSymbol}, module_symbol::ModuleSymbol,
namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol,
root_symbol::RootSymbol, symbol_mgr::{ContentSymbols, SectionIndex,
SectionRange, SymbolMgr, iter_symbol_keys}, variable_symbol::VariableSymbol,
xml_file_symbol::XmlFileSymbol }, xml_data::OdooData}, threads::SessionInfo, utils::{PathSanitizer,
compare_semver}, weak_hash_set::WeakSet};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    pub fn unwrap_package_key(&self) -> PackageKey {
        match self {
            SymbolKey::Package(k) => *k,
            _ => panic!("Not a PackageKey"),
        }
    }

    pub fn unwrap_namespace_key(&self) -> NamespaceKey {
        match self {
            SymbolKey::Namespace(k) => *k,
            _ => panic!("Not a NamespaceKey"),
        }
    }

}

/// @arena: temporary. symbol_rc.borrow() -> get_sym!(symbol_key).
/// Assumes `symbol_table` is in scope
macro_rules! get_sym {
    ($st:expr, $key:expr) => {
        $st.get_symbol_view($key).expect("valid key (formerly Rc)")
    };
}

pub(crate) use get_sym;

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

    pub fn name(&self) -> &OYarn {
        match self {
            Self::Root(s) => &s.name,
            Self::DiskDir(s) => &s.name,
            Self::Namespace(s) => &s.name,
            Self::Package(p) => p.name(),
            Self::File(f) => &f.name,
            Self::Compiled(c) => &c.name,
            Self::Class(c) => &c.name,
            Self::Function(f) => &f.name,
            Self::Variable(v) => &v.name,
            Self::XmlFileSymbol(x) => &x.name,
            Self::CsvFileSymbol(c) => &c.name,
        }
    }

    pub fn doc_string(&self) -> &Option<String> {
        match self {
            Self::Root(_) => &None,
            Self::DiskDir(_) => &None,
            Self::Namespace(_) => &None,
            Self::Package(_) => &None,
            Self::File(_) => &None,
            Self::Compiled(_) => &None,
            Self::Class(c) => &c.doc_string,
            Self::Function(f) => &f.doc_string,
            Self::Variable(v) => &v.doc_string,
            Self::XmlFileSymbol(_) => &None,
            Self::CsvFileSymbol(_) => &None,
        }
    }

    pub fn is_file_content(&self) -> bool {
        match self {
            Self::Root(_)
            | Self::Namespace(_)
            | Self::DiskDir(_)
            | Self::Package(_)
            | Self::File(_)
            | Self::Compiled(_)
            | Self::XmlFileSymbol(_)
            | Self::CsvFileSymbol(_) => false,
            Self::Class(_) | Self::Function(_) | Self::Variable(_) => true,
        }
    }

    pub fn in_workspace(&self) -> bool {
        match self {
            Self::Root(_) => false,
            Self::Namespace(n) => n.is_in_workspace(),
            Self::DiskDir(d) => d.in_workspace,
            Self::Package(PackageSymbol::Module(m)) => m.in_workspace,
            Self::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace,
            Self::File(f) => f.is_in_workspace(),
            Self::Compiled(_) => panic!(),
            Self::Class(_) => panic!(),
            Self::Function(_) => panic!(),
            Self::Variable(_) => panic!(),
            Self::XmlFileSymbol(x) => x.is_in_workspace(),
            Self::CsvFileSymbol(c) => c.is_in_workspace(),
        }
    }

    pub fn has_range(&self) -> bool {
        match self {
            Self::Root(_) => false,
            Self::DiskDir(_) => false,
            Self::Namespace(_) => false,
            Self::Package(_) => false,
            Self::File(_) => false,
            Self::Compiled(_) => false,
            Self::Class(_) => true,
            Self::Function(_) => true,
            Self::Variable(_) => true,
            Self::XmlFileSymbol(_) => false,
            Self::CsvFileSymbol(_) => false,
        }
    }

    pub fn range(&self) -> &TextRange {
        match self {
            Self::Root(_) => panic!(),
            Self::DiskDir(_) => panic!(),
            Self::Namespace(_) => panic!(),
            Self::Package(_) => panic!(),
            Self::File(_) => panic!(),
            Self::Compiled(_) => panic!(),
            Self::Class(c) => &c.range,
            Self::Function(f) => &f.range,
            Self::Variable(v) => &v.range,
            Self::XmlFileSymbol(_) => panic!(),
            Self::CsvFileSymbol(_) => panic!(),
        }
    }

    pub fn evaluations(&self) -> Option<&Vec<Evaluation>> {
        match self {
            Self::File(_) => { None },
            Self::Root(_) => { None },
            Self::Namespace(_) => { None },
            Self::DiskDir(_) => { None },
            Self::Package(_) => { None },
            Self::Compiled(_) => { None },
            Self::Class(_) => { None },
            Self::Function(f) => Some(&f.evaluations),
            Self::Variable(v) => Some(&v.evaluations),
            Self::XmlFileSymbol(_) => None,
            Self::CsvFileSymbol(_) => None,
        }
    }


    // @arena: like the original, this is not lazy iteration (might as well just return the Vec)
    pub fn all_symbols(&self) -> impl Iterator<Item = SymbolKey> + use<> {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned.
        let mut iter: Vec<SymbolKey> = Vec::new();
        match self {
            Self::File(f) => iter.extend(iter_symbol_keys(*f)),
            Self::Class(c) => iter.extend(iter_symbol_keys(*c)),
            Self::Function(f) => iter.extend(iter_symbol_keys(*f)),
            Self::Package(PackageSymbol::Module(m)) => {
                iter.extend(iter_symbol_keys(m));
                iter.extend(m.module_symbols.values());
            },
            Self::Package(PackageSymbol::PythonPackage(p)) => {
                iter.extend(iter_symbol_keys(p));
                iter.extend(p.module_symbols.values());
            },
            Self::Namespace(n) => {
                let symbols = n.directories.iter().flat_map(|d| d.module_symbols.values());
                iter.extend(symbols);
            },
            Self::Root(r) => iter.extend(r.module_symbols.values()),
            Self::DiskDir(d) => iter.extend(d.module_symbols.values()),
            _ => {}
        }
        iter.into_iter()
    }

    pub fn body_range(&self) -> &TextRange {
        match self {
            Self::Root(_) => panic!(),
            Self::DiskDir(_) => panic!(),
            Self::Namespace(_) => panic!(),
            Self::Package(_) => panic!(),
            Self::File(_) => panic!(),
            Self::Compiled(_) => panic!(),
            Self::Class(c) => &c.body_range,
            Self::Function(f) => &f.body_range,
            Self::Variable(_) => panic!(),
            Self::XmlFileSymbol(_) => panic!(),
            Self::CsvFileSymbol(_) => panic!(),
        }
    }

    // @arena: consider returning Vec<&str> instead
    pub fn paths(&self) -> Vec<String> {
        match self {
            Self::Root(r) => r.paths.clone(),
            Self::Namespace(n) => n.paths(),
            Self::DiskDir(d) => vec![d.path.clone()],
            Self::Package(p) => p.paths(),
            Self::File(f) => vec![f.path.clone()],
            Self::Compiled(c) => vec![c.path.clone()],
            Self::Class(_) => vec![],
            Self::Function(_) => vec![],
            Self::Variable(_) => vec![],
            Self::XmlFileSymbol(x) => vec![x.path.clone()],
            Self::CsvFileSymbol(c) => vec![c.path.clone()],
        }
    }

    pub fn get_symbol_first_path(&self) -> String {
        match self{
            Self::Package(p) => PathBuf::from(p.paths()[0].clone()).join("__init__.py").sanitize() + p.i_ext(),
            Self::File(f) => f.path.clone(),
            Self::DiskDir(_) => panic!("invalid symbol type to extract path"),
            Self::Root(_) => panic!("invalid symbol type to extract path"),
            Self::Namespace(_) => panic!("invalid symbol type to extract path"),
            Self::Compiled(_) => panic!("invalid symbol type to extract path"),
            Self::Class(_) => panic!("invalid symbol type to extract path"),
            Self::Function(_) => panic!("invalid symbol type to extract path"),
            Self::Variable(_) => panic!("invalid symbol type to extract path"),
            Self::XmlFileSymbol(x) => x.path.clone(),
            Self::CsvFileSymbol(c) => c.path.clone(),
        }
    }

    pub fn dependents(&self) -> &Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match self {
            Self::Root(_) => panic!("No dependencies on Root"),
            Self::Namespace(n) => n.dependents(),
            Self::DiskDir(_) => panic!("No dependencies on DiskDir"),
            Self::Package(p) => p.dependents(),
            Self::File(f) => f.dependents(),
            Self::Compiled(_) => panic!("No dependencies on Compiled"),
            Self::Class(_) => panic!("No dependencies on Class"),
            Self::Function(_) => panic!("No dependencies on Function"),
            Self::Variable(_) => panic!("No dependencies on Variable"),
            Self::XmlFileSymbol(x) => x.dependents(),
            Self::CsvFileSymbol(c) => c.dependents(),
        }
    }

    pub fn get_all_dependencies(&self, step: BuildSteps) -> Option<&Vec<Option<WeakSet<SymbolKey>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        match self {
            Self::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Self::Namespace(n) => n.get_all_dependencies(step as usize),
            Self::DiskDir(_) => panic!("There is no dependencies on DiskDir Symbol"),
            Self::Package(PackageSymbol::Module(m)) => m.get_all_dependencies(step as usize),
            Self::Package(PackageSymbol::PythonPackage(p)) => p.get_all_dependencies(step as usize),
            Self::File(f) => f.get_all_dependencies(step as usize),
            Self::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Self::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Self::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Self::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
            Self::XmlFileSymbol(x) => x.get_all_dependencies(step as usize),
            Self::CsvFileSymbol(c) => c.get_all_dependencies(step as usize),
        }
    }

    pub fn as_module_package(&self) -> &ModuleSymbol {
        match self {
            Self::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }

    pub fn as_root(&self) -> &RootSymbol {
        match self {
            Self::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }

    pub fn as_class_sym(&self) -> &ClassSymbol {
        match self {
            Self::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }


    /*
    Return a symbol that is in module symbols (symbol that represent something on disk - file, package, namespace)
     */
    pub fn get_module_symbol(&self, name: &str) -> Option<SymbolKey> {
        match self {
            Self::Namespace(n) => {
                for dir in n.directories.iter() {
                    let result = dir.module_symbols.get(name);
                    if result.is_some() {
                        return result.copied();
                    }
                }
                None
            },
            Self::Package(PackageSymbol::Module(m)) => {
                m.module_symbols.get(name).copied()
            },
            Self::Package(PackageSymbol::PythonPackage(p)) => {
                p.module_symbols.get(name).copied()
            }
            Self::Root(r) => {
                r.module_symbols.get(name).copied()
            },
            Self::DiskDir(d) => {
                d.module_symbols.get(name).copied()
            }
            _ => {None}
        }
    }

    pub fn has_modules(&self) -> bool {
        match self {
            Self::Root(_) | Self::Namespace(_) | Self::Package(_) | Self::DiskDir(_) => true,
            _ => {false}
        }
    }
    // @arena: it would be simpler to return a Vec<SymbolKey> instead
    pub fn all_module_symbol(&self) -> Box<dyn Iterator<Item = &SymbolKey> + '_> {
        match self {
            Self::Root(r) => Box::new(r.module_symbols.values()),
            Self::Namespace(n) => {
                Box::new(n.directories.iter().flat_map(|x| x.module_symbols.values()))
            },
            Self::DiskDir(d) => Box::new(d.module_symbols.values()),
            Self::Package(PackageSymbol::Module(m)) => Box::new(m.module_symbols.values()),
            Self::Package(PackageSymbol::PythonPackage(p)) => Box::new(p.module_symbols.values()),
            Self::File(_) => panic!("No module symbol on File"),
            Self::Compiled(_) => panic!("No module symbol on Compiled"),
            Self::Class(_c) => panic!("No module symbol on Class"),
            Self::Function(_) => panic!("No module symbol on Function"),
            Self::Variable(_) => panic!("No module symbol on Variable"),
            Self::XmlFileSymbol(_) => panic!("No module symbol on XmlFileSymbol"),
            Self::CsvFileSymbol(_) => panic!("No module symbol on CsvFileSymbol"),
        }
    }

    pub fn get_xml_id(&self, xml_id: &OYarn) -> Option<Vec<OdooData>> {
        match self {
            Self::XmlFileSymbol(xml_file) => xml_file.xml_ids.get(xml_id).cloned(),
            Self::Package(PackageSymbol::Module(module)) => module.xml_ids.get(xml_id).cloned(),
            Self::Package(PackageSymbol::PythonPackage(package)) => package.xml_ids.get(xml_id).cloned(),
            Self::File(file) => file.xml_ids.get(xml_id).cloned(),
            Self::CsvFileSymbol(file) => file.xml_ids.get(xml_id).cloned(),
            _ => None,
        }
    }
}


#[derive(Debug)]
pub struct SymbolTable {
    // slotmaps per symbol type
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
    // external symbols
    pub ext_symbols: ExtSymbolStore,
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
            ext_symbols: ExtSymbolStore::new(),
        }
    }

    pub fn pre_allocate(&mut self) {
        self.files.reserve(7000);
        self.packages.reserve(2200);
        self.classes.reserve(14000);
        self.functions.reserve(80000);
        self.variables.reserve(450000);
        self.xml_files.reserve(3200);
    }


    pub fn remove(&mut self, key: SymbolKey) {
        self.ext_symbols.remove(key);
        match key {
            SymbolKey::Root(k) => { self.roots.remove(k); }
            SymbolKey::DiskDir(k) => { self.disk_dirs.remove(k); }
            SymbolKey::Namespace(k) => { self.namespaces.remove(k); }
            SymbolKey::Package(k) => { self.packages.remove(k); }
            SymbolKey::File(k) => { self.files.remove(k); }
            SymbolKey::Compiled(k) => { self.compiled.remove(k); }
            SymbolKey::Class(k) => { self.classes.remove(k); }
            SymbolKey::Function(k) => { self.functions.remove(k); }
            SymbolKey::Variable(k) => { self.variables.remove(k); }
            SymbolKey::XmlFile(k) => { self.xml_files.remove(k); }
            SymbolKey::CsvFile(k) => { self.csv_files.remove(k); }
        }
    }

    // pub fn insert_variable(&mut self, symbol: VariableSymbol) -> SymbolKey {
    //     let key = self.variables.insert(symbol);
    //     SymbolKey::Variable(key)
    // }

    // pub fn insert_function(&mut self, symbol: FunctionSymbol) -> SymbolKey {
    //     let key = self.functions.insert(symbol);
    //     SymbolKey::Function(key)
    // }

    pub fn get_symbol_view(&self, key: SymbolKey) -> Option<SymbolView<'_>> {
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

    // @arena get_symbol + expect is the equivalent of upgrade + unwrap on a weak ref
    // @arena, to check possibly weird things:
    // - loop stops if symbol has no parent, without including it.
    fn get_tree_helper(&self, symbol_key: SymbolKey) -> (Tree, Option<RootKey>) {
        let mut tree = (vec![], vec![]);
        let mut current_key = symbol_key;
        let mut current_sym = self.get_symbol_view(current_key).expect("valid key");
        while current_sym.typ() != SymType::ROOT && current_sym.parent().is_some() {
            if current_sym.is_file_content() {
                tree.1.insert(0, current_sym.name().clone());
            } else {
                tree.0.insert(0, current_sym.name().clone());
            }
            current_key = current_sym.parent().unwrap();
            current_sym = self.get_symbol_view(current_key).expect("valid key");
        }
        let root = match current_key {
            SymbolKey::Root(root_key) => Some(root_key),
            _ => None,
        };
        (tree, root)
    }


    pub fn get_tree(&self, symbol_key: SymbolKey) -> Tree {
        self.get_tree_helper(symbol_key).0
    }

    pub fn get_tree_and_entry(&self, symbol_key: SymbolKey) -> (Tree, Option<Rc<RefCell<EntryPoint>>>) {
        let (tree, root_key) = self.get_tree_helper(symbol_key);
        let entry = root_key
            .and_then(|rk| self.roots.get(rk).expect("valid key").entry_point.clone());
        (tree, entry)
    }

    /**
     * Return the tree without the entrypoint tree.
     * As long as the tree starts with the entrypoint tree,
     * otherwise return the full tree, even if it is related to an entrypoint.
     * Which is possible due to relative imports e.g. `from ..module import X`.
     */
    pub fn get_local_tree(&self, symbol_key: SymbolKey) -> Tree {
        let (mut tree, entry) = self.get_tree_and_entry(symbol_key);
        if let Some(entry) = entry && tree.0.starts_with(&entry.borrow().tree) {
            tree.0.drain(0..entry.borrow().tree.len());
        }
        tree
    }

    // @arena
    // formerly a method on Symbol, so valid target expected
    // original code unwrapped the upgrade() of weak without checking
    pub fn get_in_parents(&self, target: SymbolKey, sym_types: &[SymType], stop_same_file: bool) -> Option<SymbolKey> {
        let target_symbol = self.get_symbol_view(target).expect("valid key");
        let target_type = target_symbol.typ();

        if sym_types.contains(&target_type) {
            return Some(target);
        }
        if stop_same_file && matches!(target_type, SymType::FILE | SymType::PACKAGE(_)) {
            return None;
        }
        let Some(parent) = target_symbol.parent() else {
            return None;
        };
        return self.get_in_parents(parent, sym_types, stop_same_file);
    }


    pub fn get_root(&self, target: SymbolKey) -> Option<SymbolKey> {
        self.get_in_parents(target, &[SymType::ROOT], false)
    }

    pub fn get_entry(&self, target: SymbolKey) -> Option<Rc<RefCell<EntryPoint>>> {
        self.get_root(target)
            .and_then(|root_key| self.get_symbol_view(root_key))
            .and_then(|root_symbol| root_symbol.as_root().entry_point.clone())
    }

    pub fn get_file(&self, target: SymbolKey) -> Option<SymbolKey> {
        self.get_in_parents(
            target,
            &[
                SymType::FILE,
                SymType::PACKAGE(PackageType::PYTHON_PACKAGE),
                SymType::PACKAGE(PackageType::MODULE),
                SymType::XML_FILE,
                SymType::CSV_FILE,
            ],
            false,
        )
    }
    pub fn parent_file_or_function(&self, target: SymbolKey) -> Option<SymbolKey> {
        self.get_in_parents(
            target,
            &[
                SymType::FILE,
                SymType::PACKAGE(PackageType::PYTHON_PACKAGE),
                SymType::PACKAGE(PackageType::MODULE),
                SymType::FUNCTION,
            ],
            false,
        )
    }

    ///return true if to_test is in parents of symbol or equal to it.
    /// @arena: originally Rc's
    pub fn is_symbol_in_parents(&self, symbol: SymbolKey, to_test: SymbolKey) -> bool {
        self.has_in_parents(symbol, to_test, false)
    }

    /// @arena: formely has_rc_in_parent
    pub fn has_in_parents(&self, symbol: SymbolKey, to_test: SymbolKey, stop_same_file: bool) -> bool {
        if symbol == to_test {
            return true;
        }
        let symbol_view = self.get_symbol_view(symbol).expect("valid key");
        if stop_same_file && matches!(symbol_view.typ(), SymType::FILE | SymType::PACKAGE(_)) {
            return false;
        }
        let Some(parent) = symbol_view.parent() else {
            return false;
        };
        self.is_symbol_in_parents(parent, to_test)
    }

    // Formerly called like self.find_module on a Symbol after borrowing the Rc/RefCell
    // Now called directly with the key
    // @arena: compare with get_in_parents, and chose an approach (trust the key or not)
    // Consider just calling get_in_parents
    /// @arena: this should return a ModuleKey (after spliting it from PackageKey)
    pub fn find_module(&self, key: impl Into<SymbolKey>) -> Option<SymbolKey> {
        let key = key.into();
        let symbol = self.get_symbol_view(key)?;
        if let SymbolView::Package(PackageSymbol::Module(_)) = symbol {
            return Some(key);
        }
        return self.find_module(symbol.parent()?);
    }

    // ========= former SymbolMgr trait methods =========

    /**
     * Return all symbol before the given position that match the name in the body of the symbol
     */
    /// @arena: the one from Symbol
    pub fn get_content_symbol(&self, target: SymbolKey, name: &str, position: u32) -> ContentSymbols {
        match target {
            SymbolKey::Class(_)
            | SymbolKey::File(_)
            | SymbolKey::Package(_)
            | SymbolKey::Function(_) => self._get_content_symbol(target, name, position),
            _ => ContentSymbols::default(),
        }
    }

    ///Return all the symbols that are valid as last declaration for the given position
    /// @arena: the one from SymbolMgr trait
    fn _get_content_symbol(&self, target: SymbolKey, name: &str, position: u32) -> ContentSymbols {
        let target_sym_mgr = self.get_as_symbol_mgr(target);
        let sections = target_sym_mgr.get_symbols().get(name);
        let mut content = if let Some(sections) = sections {
            let section: SectionRange = target_sym_mgr.get_section_for(position);
            self._get_loc_symbol(target_sym_mgr, sections, position, &SectionIndex::INDEX(section.index), &mut HashSet::new())
        } else {
            ContentSymbols::default()
        };
        let ext_sym = self.get_ext_symbol(target, name);
        if ext_sym.len() > 1 {
            content.symbols.extend(ext_sym.iter().cloned());
            content.always_defined = true;
        }
        content
    }

     ///given all the sections of a symbol and a position, return all the Symbols that can represent the symbol
     /// @arena: rename this method (at least remove underscore): formerly in SymbolMgr trait, thus public
    pub fn _get_loc_symbol(&self, target: &dyn SymbolMgr, map: &HashMap<u32, Vec<SymbolKey>>, position: u32, index: &SectionIndex, acc: &mut HashSet<u32>) -> ContentSymbols {
        let mut res = ContentSymbols::default();
        match index {
            SectionIndex::NONE => { return res; },
            SectionIndex::INDEX(index) => {
                if acc.contains(index){
                    res.always_defined = true;
                    return res;
                }
                let section = target.get_sections().get(*index as usize).unwrap();
                //take index and try to find an evaluation. if no evaluation is found, search in previous index, and mix evaluation if there is multiple precedences
                if let Some(symbols) = map.get(index) {
                    for &sym_key in symbols.iter().rev() {
                        let loc_sym = self.get_symbol_view(sym_key).expect("valid key");
                        if loc_sym.range().start().to_u32() < position {
                            res.symbols.push(sym_key);
                            break;
                        }
                    }
                }
                acc.insert(*index);
                if !res.symbols.is_empty() {
                    res.always_defined = true;
                    return res;
                }
                res = self._get_loc_symbol(target, map, position, &section.previous_indexes, acc);
            },
            SectionIndex::OR(indexes) => {
                if indexes.is_empty() {
                    unreachable!("Or indexes should not be empty")
                }
                res.always_defined = true;
                for index in indexes.iter() {
                    let sub_result = self._get_loc_symbol(target, map, position, index, acc);
                    res.symbols.extend(sub_result.symbols);
                    res.always_defined = res.always_defined && sub_result.always_defined;
                }
            }
        }
        res
    }

    /// Return all symbols before the given position that are visible in the body of this symbol.
    // @arena The one from Symbol
    pub fn get_all_visible_symbols(&self, target: SymbolKey, name_prefix: &String, position: u32) -> HashMap<OYarn, Vec<SymbolKey>> {
        match target {
            SymbolKey::Class(_)
            | SymbolKey::File(_)
            | SymbolKey::Package(_)
            | SymbolKey::Function(_) => self._get_all_visible_symbols(target, name_prefix, position),
            _ => HashMap::new(),
        }
    }

    // @arena The one from SymbolMgr trait
    fn _get_all_visible_symbols(&self, target: SymbolKey, name_prefix: &String, position: u32) -> HashMap<OYarn, Vec<SymbolKey>> {
        let target_sym_mgr = self.get_as_symbol_mgr(target);
        let mut result = HashMap::new();
        let current_section = target_sym_mgr.get_section_for(position);
        let current_index = SectionIndex::INDEX(current_section.index);

        for (name, section_map) in target_sym_mgr.get_symbols().iter() {
            if !name.starts_with(name_prefix) {
                continue;
            }
            let mut seen = HashSet::new();
            let content = self._get_loc_symbol(target_sym_mgr, section_map, position, &current_index, &mut seen);

            if !content.symbols.is_empty() {
                result.insert(name.clone(), content.symbols);
            }
        }
        result
    }

    /**
     * Return a symbol that can be called from outside of the body of the symbol
     */
    pub fn get_sub_symbol(&self, target: SymbolKey, name: &str, position: u32) -> ContentSymbols {
        match target {
            SymbolKey::Class(_) | SymbolKey::File(_) | SymbolKey::Package(_) => {
                self._get_content_symbol(target, name, position)
            },
            SymbolKey::Function(_) | SymbolKey::Namespace(_) => ContentSymbols {
                symbols: self.get_ext_symbol(target, name),
                always_defined: true,
            },
            _ => ContentSymbols::default(),
        }
    }

    /// get a Symbol that has the same given range and name
    /// @arena: could use symbol_mgr trait
    pub fn get_positioned_symbol(&self, target: SymbolKey, name: &OYarn, range: &TextRange) -> Option<SymbolKey> {
        let target_sym = self.get_symbol_view(target).expect("valid key");
        if let Some(symbols) = match target_sym {
            SymbolView::Class(c) => { c.symbols.get(name) },
            SymbolView::File(f) => {f.symbols.get(name)},
            SymbolView::Function(f) => {f.symbols.get(name)},
            SymbolView::Package(PackageSymbol::Module(m)) => {m.symbols.get(name)},
            SymbolView::Package(PackageSymbol::PythonPackage(p)) => {p.symbols.get(name)},
            _ => {None}
        } {
            for sym_list in symbols.values() {
                for &key in sym_list.iter() {
                    let sym = self.get_symbol_view(key).expect("valid key");
                    if sym.range().start() == range.start() {
                        return Some(key);
                    }
                }
            }
        }
        None
    }

    // @arena: there's probably a bug here (return in last loop)
    pub fn get_symbol(&self, target: SymbolKey, tree: &Tree, position: u32) -> Vec<SymbolKey> {
        let target_sym = self.get_symbol_view(target).expect("valid key");
        let symbol_tree_files: &Vec<OYarn> = &tree.0;
        let symbol_tree_content: &Vec<OYarn> = &tree.1;
        let mut iter_sym: Vec<SymbolKey>;
        if symbol_tree_files.len() != 0 {
            let _mod_iter_sym = target_sym.get_module_symbol(&symbol_tree_files[0]);
            if _mod_iter_sym.is_none() {
                return vec![];
            }
            iter_sym = vec![_mod_iter_sym.unwrap()];
            if symbol_tree_files.len() > 1 {
                for fk in symbol_tree_files[1..symbol_tree_files.len()].iter() {
                    if let Some(s) = self.get_symbol_view(*iter_sym.last().unwrap()).expect("valid key").get_module_symbol(fk) {
                        iter_sym = vec![s];
                    } else {
                        return vec![];
                    }
                }
            }
            if symbol_tree_content.len() != 0 {
                for fk in symbol_tree_content.iter() {
                    if iter_sym.len() > 1 {
                        trace!("TODO: explore all implementation possibilities");
                    }
                    let _iter_sym = self.get_sub_symbol(iter_sym[0], fk, position);
                    iter_sym = _iter_sym.symbols;
                    if iter_sym.is_empty() {
                        return vec![];
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 {
                return vec![];
            }
            iter_sym = self.get_sub_symbol(target, &symbol_tree_content[0], position).symbols;
            if iter_sym.is_empty() {
                return vec![];
            }
            if symbol_tree_content.len() > 1 {
                if iter_sym.len() > 1 {
                    trace!("TODO: explore all implementation possibilities");
                }
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    let _iter_sym = self.get_sub_symbol(iter_sym[0], fk, position);
                    iter_sym = _iter_sym.symbols;
                    // @arena: this is a loop that run only once!
                    return iter_sym;
                }
            }
        }
        iter_sym
    }

    /* return the Symbol (class, function or file) the closest to the given offset */
    pub fn get_scope_symbol(&self, file: SymbolKey, offset: u32, is_param: bool) -> SymbolKey {
        let mut result = file;
        let file_sym_mgr = self.get_as_symbol_mgr(file); // formely Rc (strong)
        let section_id = file_sym_mgr.get_section_for(offset);
        for (_, sym_map) in file_sym_mgr.get_symbols() {
            match sym_map.get(&section_id.index) {
                Some(symbols) => {
                    for &key in symbols {
                        let symbol = self.get_symbol_view(key).expect("valid key");
                        match symbol {
                            SymbolView::Class(c) => {
                                let range = match is_param {
                                    true => c.range.start().to_u32(),
                                    false => c.body_range.start().to_u32(),
                                };
                                if range <= offset && c.body_range.end().to_u32() > offset {
                                    result = self.get_scope_symbol(key, offset, is_param);
                                }
                            },
                            SymbolView::Function(f) => {
                                let range = match is_param {
                                    true => f.range.start().to_u32(),
                                    false => f.body_range.start().to_u32(),
                                };
                                if range <= offset && f.body_range.end().to_u32() > offset {
                                    result = self.get_scope_symbol(key, offset, is_param);
                                }
                            }
                            _ => {}
                        }
                    }
                },
                None => {}
            }
        }
        result
    }


    // ==== ClassSymbol methods

    /// @arena: no callers/ dead code??
    pub fn is_class_descriptor(&self, key: ClassKey) -> bool {
        for &sym_key in self.get_content_symbol(key.into(), "__get__", u32::MAX).symbols.iter() {
            if let SymbolKey::Function(_) = sym_key {
                return true;
            }
        }
        false
    }


    // ==== FunctionSymbol methods

    /// Return true if a previous implementation has the @overload decorator or has it itself
    /// @arena: formerly a method in FunctionSymbol
    /// @arena: move back to FunctionSymbol, as assoc function
    pub fn is_func_overloaded(&self, key: FunctionKey) -> bool {
        let func = self.functions.get(key).expect("valid key");
        if func.is_overloaded {
            return true;
        }
        let Some(parent_key) = func.parent else {
            return false;
        };
        // @arena: Equivalent of if Some(parent) = parent_weak.upgrade()
        if !self.contains_key(parent_key) {
            return false;
        }
        let previous_defs = self.get_content_symbol(parent_key, &func.name, func.range.start().to_u32()).symbols;
        if let Some(SymbolKey::Function(k)) = previous_defs.last() {
            // @arena: previous_defs is [Rc] (strong) originally
            return self.functions.get(*k).expect("valid key").is_overloaded;
        }
        false
    }

    // @arena: possible undeflow bug/panic: subtractions followed by cast to u32
    /* Given a call of this function and an index, return the corresponding parameter definition */
    pub fn get_indexed_arg_in_call(&self, key: FunctionKey, call: &ExprCall, index: u32, is_on_instance: Option<bool>) -> Option<&Argument> {
        if self.is_func_overloaded(key) {
            return None;
        }
        let func = self.functions.get(key).expect("valid key");
        let mut call_arg_keyword = None;
        if index > (call.arguments.args.len()-1) as u32 {
            call_arg_keyword = call.arguments.keywords.get((index - call.arguments.args.len() as u32) as usize);
        }
        let arg_index = if is_on_instance.unwrap_or(false) {
            index + 1
        } else {
            index
        };

        if let Some(keyword) = call_arg_keyword {
            for arg in func.args.iter() {
                let arg_sym = self.get_symbol_view(arg.symbol).expect("valid key");
                if *arg_sym.name() == keyword.arg.as_ref().unwrap().id {
                    return Some(arg);
                }
            }
        } else {
            return func.args.get(arg_index as usize);
        }
        None
    }


    // ======== Dependencies management

    // @arena: consider using get_unchecked_mut for verified keys (in private methods)
    fn dependencies_mut(&mut self, valid_key: SymbolKey) -> &mut Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match valid_key {
            SymbolKey::File(k) => &mut self.files[k].dependencies,
            SymbolKey::Namespace(k) => &mut self.namespaces[k].dependencies,
            SymbolKey::XmlFile(k) => &mut self.xml_files[k].dependencies,
            SymbolKey::CsvFile(k) => &mut self.csv_files[k].dependencies,
            SymbolKey::Package(k) => self.packages[k].dependencies_as_mut(),
            SymbolKey::Root(_)
            | SymbolKey::DiskDir(_)
            | SymbolKey::Compiled(_)
            | SymbolKey::Class(_)
            | SymbolKey::Function(_)
            | SymbolKey::Variable(_) => panic!("No dependencies on {:?}", valid_key),
        }
    }

    fn dependents_as_mut(&mut self, valid_key: SymbolKey) -> &mut Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match valid_key {
            SymbolKey::Namespace(n) => &mut self.namespaces[n].dependents,
            SymbolKey::File(f) => &mut self.files[f].dependents,
            SymbolKey::XmlFile(x) => &mut self.xml_files[x].dependents,
            SymbolKey::CsvFile(c) => &mut self.csv_files[c].dependents,
            SymbolKey::Package(p) => self.packages[p].dependents_as_mut(),
            SymbolKey::Root(_)
            | SymbolKey::DiskDir(_)
            | SymbolKey::Compiled(_)
            | SymbolKey::Class(_)
            | SymbolKey::Function(_)
            | SymbolKey::Variable(_) => panic!("No dependencies on {:?}", valid_key),
        }
    }

    /**Add a symbol as dependency on the step of the other symbol for the build level.
    * -> The build of the 'step' of 'target' requires the build of 'dep_level' of 'dependency' to be done */
    // @arena: do we need protection against self-dependencies?
    pub fn add_dependency(&mut self, target: SymbolKey, dependency: SymbolKey, step:BuildSteps, dep_level:BuildSteps) {
        if step == BuildSteps::SYNTAX || dep_level == BuildSteps::SYNTAX {
            panic!("Can't add dependency for syntax step")
        }
        if dep_level > step {
            panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
        }
        let target_sym = self.get_symbol_view(target).expect("valid key");
        let dependency_sym = self.get_symbol_view(dependency).expect("valid key");
        if !target_sym.in_workspace() || !dependency_sym.in_workspace() {
            return;
        }
        let step_i = step as usize;
        let level_i = dep_level as usize;

        // register `depends_on` as a dependency of `target`
        self.dependencies_mut(target)[step_i][level_i]
            .get_or_insert_with(WeakSet::new)
            .insert(dependency);

        // register `target` as a dependent of `depends_on`
        self.dependents_as_mut(dependency)[level_i][step_i - level_i]
            .get_or_insert_with(WeakSet::new)
            .insert(target);
    }

    /* Helper to merge dependencies eval_from_ast will fill when called. To be called on a file/package... */
    pub fn insert_dependencies(&mut self, target: SymbolKey, deps: &Vec<Vec<SymbolKey>>, current_step: BuildSteps) {
        for (step, dependencies) in deps.iter().enumerate() {
            let dep_level = BuildSteps::from(step as i32);
            for &dependency in dependencies.iter() {
                if target != dependency {
                    self.add_dependency(target, dependency, current_step, dep_level);
                }
            }
        }
    }

    // @arena TODO: convert Rc<RefCell<Model>> too! Then make this sync_odoo method (uses symbol table and model table)
    pub fn add_model_dependencies(&mut self, target: SymbolKey, model: &Rc<RefCell<Model>>) {
        match target {
            SymbolKey::Package(p) => {
                match self.packages.get_mut(p).expect("valid key") {
                    PackageSymbol::Module(m) => {
                        m.model_dependencies.insert(model.clone());
                    }
                    PackageSymbol::PythonPackage(p) => {
                        p.model_dependencies.insert(model.clone());
                    }
                }
            },
            SymbolKey::File(f) => {
                let file = self.files.get_mut(f).expect("valid key");
                file.model_dependencies.insert(model.clone());
            },
            SymbolKey::Function(f) => {
                let func = self.functions.get_mut(f).expect("valid key");
                func.model_dependencies.insert(model.clone());
            }
            _ => { return; }
        }
        model.borrow_mut().add_dependent(target);
    }

    pub fn build_status(&self, target: SymbolKey, step: BuildSteps) -> BuildStatus {
        debug_assert!(self.contains_key(target)); // expect valid key (self in Symbol method)
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::Package(k) => self.packages[k].build_status(step),
            SymbolKey::File(k) => self.files[k].build_status(step),
            SymbolKey::Compiled(_) => todo!(),
            SymbolKey::Class(_) => todo!(),
            SymbolKey::Function(k) => self.functions[k].build_status(step),
            SymbolKey::Variable(_) => todo!(),
            SymbolKey::XmlFile(k) => self.xml_files[k].build_status(step),
            SymbolKey::CsvFile(k) => self.csv_files[k].build_status(step),
        }
    }

    pub fn set_build_status(&mut self, target: SymbolKey, step: BuildSteps, status: BuildStatus) {
        debug_assert!(self.contains_key(target)); // expect valid key (self in Symbol method)
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::Package(k) => self.packages[k].set_build_status(step, status),
            SymbolKey::File(k) => self.files[k].set_build_status(step, status),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(_) => panic!(),
            SymbolKey::Function(k) => self.functions[k].set_build_status(step, status),
            SymbolKey::Variable(_) => todo!(),
            SymbolKey::XmlFile(k) => self.xml_files[k].set_build_status(step, status),
            SymbolKey::CsvFile(k) => self.csv_files[k].set_build_status(step, status),
        }
    }

    pub fn previous_step_done(&self, target: SymbolKey, step: BuildSteps) -> bool {
        if step == BuildSteps::SYNTAX {
            panic!("Can't check previous step for syntax step")
        }
        for i in 0 .. step as usize {
            if self.build_status(target, BuildSteps::from(i as i32)) != BuildStatus::DONE {
                return false;
            }
        }
        true
    }

    pub fn invalidate_sub_functions(&mut self, target: SymbolKey) {
        if matches!(target, SymbolKey::File(_) | SymbolKey::Package(_)) {
            for func_key in self.iter_inner_functions(target) {
                let func = self.functions.get_mut(func_key).expect("valid key"); // Rc's in the original code
                func.evaluations.clear();
                func.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
                func.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            }
        }
    }

    /**
     * Only browse file content, do not use on namespace or packages to browse disk
     * return a list of functions under Class symbol
     */
    pub fn iter_inner_functions(&self, key: SymbolKey) -> Vec<FunctionKey> {
        let mut res = vec![];

        fn iter_recursive(table: &SymbolTable, key: SymbolKey, res: &mut Vec<FunctionKey>) {
            match table.get_symbol_view(key).expect("valid key") {
                SymbolView::Class(c) => {
                    for child_key in iter_symbol_keys(c) {
                        if let SymbolKey::Function(fk) = child_key {
                            res.push(*fk);
                        }
                    }
                },
                SymbolView::File(f) => {
                    for child_key in iter_symbol_keys(f) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolView::Function(f) => {
                    for child_key in iter_symbol_keys(f) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolView::DiskDir(_)
                | SymbolView::Root(_)
                | SymbolView::Namespace(_)
                | SymbolView::Package(_)
                | SymbolView::Compiled(_)
                | SymbolView::Variable(_)
                | SymbolView::XmlFileSymbol(_)
                | SymbolView::CsvFileSymbol(_) => {},
            }
        }

        iter_recursive(self, key, &mut res);
        res
    }

    pub fn iter_classes(&self, key: SymbolKey) -> Vec<ClassKey> {
        let mut res = vec![];

        fn iter_recursive(table: &SymbolTable, key: SymbolKey, res: &mut Vec<ClassKey>) {
            match key {
                SymbolKey::Class(c) => {
                    res.push(c);
                    let class_sym = table.classes.get(c).expect("valid key");
                    for child_key in iter_symbol_keys(class_sym) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::File(f) => {
                    let file_sym = table.files.get(f).expect("valid key");
                    for child_key in iter_symbol_keys(file_sym) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::Function(f) => {
                    let func_sym = table.functions.get(f).expect("valid key");
                    for child_key in iter_symbol_keys(func_sym) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::DiskDir(_)
                | SymbolKey::Root(_)
                | SymbolKey::Namespace(_)
                | SymbolKey::Package(_)
                | SymbolKey::Compiled(_)
                | SymbolKey::Variable(_)
                | SymbolKey::XmlFile(_)
                | SymbolKey::CsvFile(_) => {},
            }
        }

        iter_recursive(self, key, &mut res);

        res
    }

    // @arena: originally method on EvaluationSymbolPtr
    pub fn upgrade_weak(&self, eval_ptr: &EvaluationSymbolPtr) -> Option<SymbolKey> {
        match eval_ptr {
            EvaluationSymbolPtr::WEAK(w) => self.upgrade(w.weak),
            _ => None,
        }
    }

    // @arena: originally method on EvaluationSymbolPtr
    pub fn is_expired_if_weak(&self, eval_ptr: &EvaluationSymbolPtr) -> bool {
        match eval_ptr {
            EvaluationSymbolPtr::WEAK(w) => w.weak.is_expired(self),
            _ => false,
        }
    }

    pub fn set_in_workspace(&mut self, target: SymbolKey, in_workspace: bool) {
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(n) => self.namespaces[n].set_in_workspace(in_workspace),
            SymbolKey::DiskDir(d) => { self.disk_dirs[d].in_workspace = in_workspace; },
            SymbolKey::Package(p) => match &mut self.packages[p] {
                PackageSymbol::Module(m) => m.set_in_workspace(in_workspace),
                PackageSymbol::PythonPackage(p) => p.set_in_workspace(in_workspace),
            }
            SymbolKey::File(f) => self.files[f].set_in_workspace(in_workspace),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(_) => panic!(),
            SymbolKey::Function(_) => panic!(),
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(x) => self.xml_files[x].set_in_workspace(in_workspace),
            SymbolKey::CsvFile(c) => self.csv_files[c].set_in_workspace(in_workspace),
        }
    }

    pub fn get_noqas(&self, target: SymbolKey) -> NoqaInfo {
        match target {
            SymbolKey::File(f) => self.files[f].noqas.clone(),
            SymbolKey::Package(p) => match &self.packages[p] {
                PackageSymbol::Module(m) => m.noqas.clone(),
                PackageSymbol::PythonPackage(p) => p.noqas.clone(),
            }
            SymbolKey::DiskDir(_) => panic!("get_noqas called on DiskDir"),
            SymbolKey::Function(f) => self.functions[f].noqas.clone(),
            SymbolKey::Root(_) => panic!("get_noqas called on Root"),
            SymbolKey::Namespace(_) => panic!("get_noqas called on Namespace"),
            SymbolKey::Compiled(_) => panic!("get_noqas called on Compiled"),
            SymbolKey::Class(c) => self.classes[c].noqas.clone(),
            SymbolKey::Variable(_) => panic!("get_noqas called on Variable"),
            SymbolKey::XmlFile(x) => self.xml_files[x].noqas.clone(),
            SymbolKey::CsvFile(c) => self.csv_files[c].noqas.clone(),
        }
    }

    pub fn set_noqas(&mut self, target: SymbolKey, noqa: NoqaInfo) {
        match target {
            SymbolKey::File(f) => self.files[f].noqas = noqa,
            SymbolKey::DiskDir(_) => panic!("set_noqas called on DiskDir"),
            SymbolKey::Package(p) => match &mut self.packages[p] {
                PackageSymbol::Module(m) => m.noqas = noqa,
                PackageSymbol::PythonPackage(p) => p.noqas = noqa,
            }
            SymbolKey::Function(f) => self.functions[f].noqas = noqa,
            SymbolKey::Root(_) => panic!("set_noqas called on Root"),
            SymbolKey::Namespace(_) => panic!("set_noqas called on Namespace"),
            SymbolKey::Compiled(_) => panic!("set_noqas called on Compiled"),
            SymbolKey::Class(c) => self.classes[c].noqas = noqa,
            SymbolKey::Variable(_) => panic!("set_noqas called on Variable"),
            SymbolKey::XmlFile(x) => self.xml_files[x].noqas = noqa,
            SymbolKey::CsvFile(c) => self.csv_files[c].noqas = noqa,
        }
    }

    pub fn get_processed_text_hash(&self, target: SymbolKey) -> u64 {
        match target {
            SymbolKey::File(f) => self.files[f].processed_text_hash,
            SymbolKey::Package(p) => match &self.packages[p] {
                PackageSymbol::Module(m) => m.processed_text_hash,
                PackageSymbol::PythonPackage(p) => p.processed_text_hash,
            }
            SymbolKey::DiskDir(_) => panic!("get_processed_text_hash called on DiskDir"),
            SymbolKey::Function(_) => panic!("get_processed_text_hash called on Function"),
            SymbolKey::Root(_) => panic!("get_processed_text_hash called on Root"),
            SymbolKey::Namespace(_) => panic!("get_processed_text_hash called on Namespace"),
            SymbolKey::Compiled(_) => panic!("get_processed_text_hash called on Compiled"),
            SymbolKey::Class(_) => panic!("get_processed_text_hash called on Class"),
            SymbolKey::Variable(_) => panic!("get_processed_text_hash called on Variable"),
            SymbolKey::XmlFile(x) => self.xml_files[x].processed_text_hash,
            SymbolKey::CsvFile(c) => self.csv_files[c].processed_text_hash,
        }
    }

    pub fn set_processed_text_hash(&mut self, target: SymbolKey, hash: u64) {
        match target {
            SymbolKey::File(f) => self.files[f].processed_text_hash = hash,
            SymbolKey::DiskDir(_) => panic!("set_processed_text_hash called on DiskDir"),
            SymbolKey::Package(p) => match &mut self.packages[p] {
                PackageSymbol::Module(m) => m.processed_text_hash = hash,
                PackageSymbol::PythonPackage(p) => p.processed_text_hash = hash,
            },
            SymbolKey::Function(_) => panic!("set_processed_text_hash called on Function"),
            SymbolKey::Root(_) => panic!("set_processed_text_hash called on Root"),
            SymbolKey::Namespace(_) => panic!("set_processed_text_hash called on Namespace"),
            SymbolKey::Compiled(_) => panic!("set_processed_text_hash called on Compiled"),
            SymbolKey::Class(_) => panic!("set_processed_text_hash called on Class"),
            SymbolKey::Variable(_) => panic!("set_processed_text_hash called on Variable"),
            SymbolKey::XmlFile(x) => self.xml_files[x].processed_text_hash = hash,
            SymbolKey::CsvFile(c) => self.csv_files[c].processed_text_hash = hash,
        }
    }

    pub fn not_found_paths(&self, target: SymbolKey) -> &Vec<(BuildSteps, Vec<OYarn>)> {
        static EMPTY_VEC: Vec<(BuildSteps, Vec<OYarn>)> = Vec::new();
        match target {
            SymbolKey::File(f) => { &self.files[f].not_found_paths },
            SymbolKey::Root(_) => { &EMPTY_VEC },
            SymbolKey::Namespace(_) => { &EMPTY_VEC },
            SymbolKey::DiskDir(_) => { &EMPTY_VEC },
            SymbolKey::Package(p) => match &self.packages[p] {
                 PackageSymbol::Module(m) => &m.not_found_paths,
                 PackageSymbol::PythonPackage(p) => &p.not_found_paths,
            },
            SymbolKey::Compiled(_) => { &EMPTY_VEC },
            SymbolKey::Class(_) => { &EMPTY_VEC },
            SymbolKey::Function(_) => { &EMPTY_VEC },
            SymbolKey::Variable(_) => &EMPTY_VEC,
            SymbolKey::XmlFile(_) => { &EMPTY_VEC },
            SymbolKey::CsvFile(_) => { &EMPTY_VEC },
        }
    }

    pub fn not_found_paths_mut(&mut self, target: SymbolKey) -> &mut Vec<(BuildSteps, Vec<OYarn>)> {
        match target {
            SymbolKey::File(f) => { &mut self.files[f].not_found_paths },
            SymbolKey::Root(_) => { panic!("no not_found_path on Root") },
            SymbolKey::Namespace(_) => { panic!("no not_found_path on Namespace") },
            SymbolKey::DiskDir(_) => { panic!("no not_found_path on DiskDir") },
            SymbolKey::Package(k) => match &mut self.packages[k] {
                PackageSymbol::Module(m) => &mut m.not_found_paths,
                PackageSymbol::PythonPackage(p) => &mut p.not_found_paths,
            }
            SymbolKey::Compiled(_) => { panic!("no not_found_path on Compiled") },
            SymbolKey::Class(_) => { panic!("no not_found_path on Class") },
            SymbolKey::Function(_) => { panic!("no not_found_path on Function") },
            SymbolKey::Variable(_) => panic!("no not_found_path on Variable"),
            SymbolKey::XmlFile(_) => { panic!("no not_found_path on XmlFileSymbol") },
            SymbolKey::CsvFile(_) => { panic!("no not_found_path on CsvFileSymbol") },
        }
    }

    pub fn not_found_models(&self, target: SymbolKey) -> Option<&HashMap<OYarn, BuildSteps>> {
        match target {
            SymbolKey::File(f) => Some(&self.files[f].not_found_models),
            SymbolKey::XmlFile(f) => Some(&self.xml_files[f].not_found_models),
            SymbolKey::Package(p) => match &self.packages[p] {
                PackageSymbol::Module(m) => Some(&m.not_found_models),
                PackageSymbol::PythonPackage(_) => None,
            },
            SymbolKey::Root(_) => None,
            SymbolKey::Namespace(_) => None,
            SymbolKey::DiskDir(_) => None,
            SymbolKey::Compiled(_) => None,
            SymbolKey::Class(_) => None,
            SymbolKey::Function(_) => None,
            SymbolKey::Variable(_) => None,
            SymbolKey::CsvFile(_) => None,
        }
    }

    pub fn not_found_models_mut(&mut self, target: SymbolKey) -> Option<&mut HashMap<OYarn, BuildSteps>> {
        match target {
            SymbolKey::File(f) => Some(&mut self.files[f].not_found_models),
            SymbolKey::XmlFile(f) => Some(&mut self.xml_files[f].not_found_models),
            SymbolKey::Package(p) => match &mut self.packages[p] {
                PackageSymbol::Module(m) => Some(&mut m.not_found_models),
                PackageSymbol::PythonPackage(_) => None,
            },
            SymbolKey::Root(_) => None,
            SymbolKey::Namespace(_) => None,
            SymbolKey::DiskDir(_) => None,
            SymbolKey::Compiled(_) => None,
            SymbolKey::Class(_) => None,
            SymbolKey::Function(_) => None,
            SymbolKey::Variable(_) => None,
            SymbolKey::CsvFile(_) => None,
        }
    }

    pub fn get_as_symbol_mgr(&self, target: SymbolKey) -> &dyn SymbolMgr {
        match target {
            SymbolKey::File(f) => &self.files[f],
            SymbolKey::Class(c) => &self.classes[c],
            SymbolKey::Function(f) => &self.functions[f],
            SymbolKey::Package(p) => match &self.packages[p] {
                PackageSymbol::Module(m) => m,
                PackageSymbol::PythonPackage(p) => p,
            },
            _ => {panic!("Not a symbol Mgr");}
        }
    }

    pub fn get_as_mut_symbol_mgr(&mut self, target: SymbolKey) -> &mut dyn SymbolMgr {
        match target {
            SymbolKey::File(f) => &mut self.files[f],
            SymbolKey::Class(c) => &mut self.classes[c],
            SymbolKey::Function(f) => &mut self.functions[f],
            SymbolKey::Package(p) => match &mut self.packages[p] {
                PackageSymbol::Module(m) => m,
                PackageSymbol::PythonPackage(p) => p,
            },
            _ => {panic!("Not a symbol Mgr");}
        }
    }

    pub fn iter_symbols(&self, target: SymbolKey) -> hash_map::Iter<'_, OYarn, HashMap<u32, Vec<SymbolKey>>> {
        match target {
            SymbolKey::File(f) => {
                self.files[f].symbols.iter()
            }
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::Package(p) => match &self.packages[p] {
                PackageSymbol::Module(m) => m.symbols.iter(),
                PackageSymbol::PythonPackage(p) => p.symbols.iter(),
            },
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(c) => {
                self.classes[c].symbols.iter()
            },
            SymbolKey::Function(f) => {
                self.functions[f].symbols.iter()
            },
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(_) => panic!(),
            SymbolKey::CsvFile(_) => panic!(),
        }
    }

    // ======= Duplicates of SymbolView methods =============
    //
    // @arena: consider removing the one from SymbolView
    pub fn evaluations(&self, target: SymbolKey) -> Option<&Vec<Evaluation>> {
        match target {
            SymbolKey::File(_) => { None },
            SymbolKey::Root(_) => { None },
            SymbolKey::Namespace(_) => { None },
            SymbolKey::DiskDir(_) => { None },
            SymbolKey::Package(_) => { None },
            SymbolKey::Compiled(_) => { None },
            SymbolKey::Class(_) => { None },
            SymbolKey::Function(f) => Some(&self.functions[f].evaluations),
            SymbolKey::Variable(v) => Some(&self.variables[v].evaluations),
            SymbolKey::XmlFile(_) => None,
            SymbolKey::CsvFile(_) => None,
        }
    }

    // @arena: consider replacing the calls to symbol view by this one
    pub fn is_external(&self, target: SymbolKey) -> bool {
        match target {
            SymbolKey::Root(_) => false,
            SymbolKey::DiskDir(d) => self.disk_dirs[d].is_external,
            SymbolKey::Namespace(n) => self.namespaces[n].is_external,
            SymbolKey::Package(p) => self.packages[p].is_external(),
            SymbolKey::File(f) => self.files[f].is_external,
            SymbolKey::Compiled(c) => self.compiled[c].is_external,
            SymbolKey::Class(c) => self.classes[c].is_external,
            SymbolKey::Function(f) => self.functions[f].is_external,
            SymbolKey::Variable(v) => self.variables[v].is_external,
            SymbolKey::XmlFile(x) => self.xml_files[x].is_external,
            SymbolKey::CsvFile(c) => self.csv_files[c].is_external,
        }
    }

    pub fn set_is_external(&mut self, target: SymbolKey, external: bool) {
        match target {
            SymbolKey::Root(_) => {},
            SymbolKey::DiskDir(d) => self.disk_dirs[d].is_external = external,
            SymbolKey::Namespace(n) => self.namespaces[n].is_external = external,
            SymbolKey::Package(p) => match &mut self.packages[p] {
                 PackageSymbol::Module(m) => m.is_external = external,
                 PackageSymbol::PythonPackage(p) => p.is_external = external,
            },
            SymbolKey::File(f) => self.files[f].is_external = external,
            SymbolKey::Compiled(c) => self.compiled[c].is_external = external,
            SymbolKey::Class(c) => self.classes[c].is_external = external,
            SymbolKey::Function(f) => self.functions[f].is_external = external,
            SymbolKey::Variable(v) => self.variables[v].is_external = external,
            SymbolKey::XmlFile(x) => self.xml_files[x].is_external = external,
            SymbolKey::CsvFile(c) => self.csv_files[c].is_external = external,
        }
    }

    pub fn name(&self, target: SymbolKey) -> &OYarn {
        match target {
            SymbolKey::Root(k) => &self.roots[k].name,
            SymbolKey::DiskDir(k) => &self.disk_dirs[k].name,
            SymbolKey::Namespace(k) => &self.namespaces[k].name,
            SymbolKey::Package(k) => self.packages[k].name(),
            SymbolKey::File(k) => &self.files[k].name,
            SymbolKey::Compiled(k) => &self.compiled[k].name,
            SymbolKey::Class(k) => &self.classes[k].name,
            SymbolKey::Function(k) => &self.functions[k].name,
            SymbolKey::Variable(k) => &self.variables[k].name,
            SymbolKey::XmlFile(k) => &self.xml_files[k].name,
            SymbolKey::CsvFile(k) => &self.csv_files[k].name,
        }
    }

    pub fn parent(&self, target: SymbolKey) -> Option<SymbolKey> {
        match target {
            SymbolKey::Root(k) => self.roots[k].parent,
            SymbolKey::DiskDir(k) => self.disk_dirs[k].parent,
            SymbolKey::Namespace(k) => self.namespaces[k].parent,
            SymbolKey::Package(k) => self.packages[k].parent(),
            SymbolKey::File(k) => self.files[k].parent,
            SymbolKey::Compiled(k) => self.compiled[k].parent,
            SymbolKey::Class(k) => self.classes[k].parent,
            SymbolKey::Function(k) => self.functions[k].parent,
            SymbolKey::Variable(k) => self.variables[k].parent,
            SymbolKey::XmlFile(x) => self.xml_files[x].parent,
            SymbolKey::CsvFile(c) => self.csv_files[c].parent,        }
    }

    // @arena: not sure if good idea. Maybe just access directly when the key type is known?
    // can be used in PythonArchBuildHooks if enabled.
    pub fn file_path(&self, target: SymbolKey) -> &str {
        match target {
            SymbolKey::File(f) => &self.files[f].path,
            SymbolKey::XmlFile(x) => &self.xml_files[x].path,
            SymbolKey::CsvFile(c) => &self.csv_files[c].path,
            _ => panic!("file_path called on non file symbol"),
        }
    }

    pub fn set_evaluations(&mut self, target: SymbolKey, data: Vec<Evaluation>) {
        match target {
            SymbolKey::File(_) => { panic!() },
            SymbolKey::Root(_) => { panic!() },
            SymbolKey::Namespace(_) => { panic!() },
            SymbolKey::DiskDir(_) => { panic!() },
            SymbolKey::Package(_) => { panic!() },
            SymbolKey::Compiled(_) => { panic!() },
            SymbolKey::Class(_) => { panic!() },
            SymbolKey::Function(f) => { self.functions[f].evaluations = data; },
            SymbolKey::Variable(v) => { self.variables[v].evaluations = data; },
            SymbolKey::XmlFile(_) => { panic!() },
            SymbolKey::CsvFile(_) => { panic!() },
        }
    }

    pub fn insert_xml_id(&mut self, target: SymbolKey, xml_id: OYarn, xml_data: OdooData) {
        match target {
            SymbolKey::File(file) => {
                self.files[file].xml_ids.entry(xml_id).or_insert(vec![]).push(xml_data);
            },
            SymbolKey::Package(p) => match &mut self.packages[p] {
                PackageSymbol::Module(module) => {
                    module.xml_ids.entry(xml_id).or_insert(vec![]).push(xml_data);
                },
                PackageSymbol::PythonPackage(package) => {
                    package.xml_ids.entry(xml_id).or_insert(vec![]).push(xml_data);
                },
            },
            _ => {}
        }
    }
}

//infer a name, given a position
pub fn infer_name(odoo: &SyncOdoo, on_symbol_key: SymbolKey, name: &String, position: Option<u32>) -> ContentSymbols {
    let symbol_table = &odoo.symbol_table;
    let results = symbol_table.get_content_symbol(on_symbol_key, name, position.unwrap_or(u32::MAX));
    if !results.symbols.is_empty() {
        return results;
    }
    let on_symbol = symbol_table.get_symbol_view(on_symbol_key).expect("valid key");
    if !matches!(&on_symbol.typ(), SymType::FILE | SymType::PACKAGE(_) | SymType::ROOT) {
        let mut parent = on_symbol.parent().unwrap();
        while let SymbolKey::Class(c) = parent {
            let class_sym = symbol_table.classes.get(c).expect("valid key");
            parent = class_sym.parent.unwrap();
        }
        // A function can reference another name from the full outer scope so no position is needed
        infer_name(odoo, parent, name, None)
    } else if on_symbol.name() != "builtins" || on_symbol.typ() != SymType::FILE {
        let builtins = odoo.get_symbol("", &(vec![Sy!("builtins")], vec![]), u32::MAX)[0];
        infer_name(odoo, builtins, name, None)
    } else {
        ContentSymbols::default()
    }
}

// @arena: make this a method of SyncOdoo?
pub fn get_main_entry_tree(session: &SessionInfo, symbol_key: SymbolKey) -> Tree {
    let symbol_table = &session.sync_odoo.symbol_table;
    let mut tree = symbol_table.get_tree(symbol_key);
    let len_first_part = tree.0.len();
    let odoo_tree = &session.sync_odoo.main_entry_tree;
    if len_first_part >= odoo_tree.len() {
        for component in odoo_tree.iter() {
            if tree.0.len() > 0 && &tree.0[0] == component {
                tree.0.remove(0);
            } else {
                return symbol_table.get_tree(symbol_key);
            }
        }
    }
    tree
}

// @arena: make this a method of SyncOdoo?
pub fn match_tree_from_any_entry(session: &SessionInfo, symbol_key: SymbolKey, tree: &Tree) -> bool {
    let symbol_table = &session.sync_odoo.symbol_table;
    let (mut self_tree, entry) = symbol_table.get_tree_and_entry(symbol_key);
    'outer: for entry in session.sync_odoo.entry_point_mgr.borrow().iter_for_import(&entry.unwrap()) {
        if entry.borrow().tree.len() > self_tree.0.len() {
            continue;
        }
        for (index, tree_el) in entry.borrow().tree.iter().enumerate() {
            if &self_tree.0[index] != tree_el {
                continue 'outer;
            }
        }
        return (self_tree.0.split_off(entry.borrow().tree.len()), self_tree.1) == *tree;
    }
    false
}

pub fn is_inheriting_from_field(session: &SessionInfo, class_key: ClassKey) -> bool {
    let tree = flatten_tree(&get_main_entry_tree(session, class_key.into()));
    if compare_semver(&session.sync_odoo.full_version, "18.0") <= Ordering::Equal {
        // @arena: originally:
        // if tree.len() == 3 && tree[0] == "odoo" && tree[1] == "fields" {
        //     if tree[2].as_str() == "Field" {
        //         return true;
        //     }
        // }
        if tree.as_slice() == ["odoo", "fields", "Field"] {
            return true;
        }

    } else {
        // @arena: originally:
        // if tree.len() == 4 && tree[0] == "odoo" && tree[1] == "orm" && (
        //         tree[2] == "fields" && tree[3] == "Field"
        // ){
        //     return true;
        // }
        if tree.as_slice() == ["odoo", "orm", "fields", "Field"] {
            return true;
        }
    }
    // Follow class inheritance
    let symbol_table = &session.sync_odoo.symbol_table;
    let class_symbol = symbol_table.classes.get(class_key).expect("valid key");
    // @arena: ClassSymbol.bases are weak refs
    for base_key in class_symbol.bases.iter().filter_map(|w| w.upgrade(symbol_table)) {
        if is_inheriting_from_field(session, base_key) {
            return true;
        }
    }
    false
}

/// @arena: this shoud probably be an associated function of ClassSymbol
pub fn is_field_class(session: &SessionInfo, symbol_key: SymbolKey) -> bool {
    // if not class return false
    let SymbolKey::Class(class_key) = symbol_key else {
        return false;
    };
    let symbol_table = &session.sync_odoo.symbol_table;
    let class_symbol = symbol_table.classes.get(class_key).expect("valid key");
    let cache = &class_symbol._is_field_class;
    if let Some(is_field_class) = *cache.borrow() {
        return is_field_class;
    }
    let result = is_field_class_uncached(session, class_key);
    cache.borrow_mut().replace(result);
    result
}

fn is_field_class_uncached(session: &SessionInfo, class_key: ClassKey) -> bool {
    let tree = &get_main_entry_tree(session, class_key.into());
    if compare_semver(session.sync_odoo.full_version.as_str(), "18.1.0") >= Ordering::Equal {
        if tree.0.len() == 3 && tree.1.len() == 1 && tree.0[0] == "odoo" && tree.0[1] == "orm" && (
                tree.0[2] == "fields_misc" && tree.1[0] == "Boolean" ||
                tree.0[2] == "fields_numeric" && tree.1[0] == "Integer" ||
                tree.0[2] == "fields_numeric" && tree.1[0] == "Float" ||
                tree.0[2] == "fields_numeric" && tree.1[0] == "Monetary" ||
                tree.0[2] == "fields_textual" && tree.1[0] == "Char" ||
                tree.0[2] == "fields_textual" && tree.1[0] == "Text" ||
                tree.0[2] == "fields_textual" && tree.1[0] == "Html" ||
                tree.0[2] == "fields_temporal" && tree.1[0] == "Date" ||
                tree.0[2] == "fields_temporal" && tree.1[0] == "Datetime" ||
                tree.0[2] == "fields_binary" && tree.1[0] == "Binary" ||
                tree.0[2] == "fields_binary" && tree.1[0] == "Image" ||
                tree.0[2] == "fields_selection" && tree.1[0] == "Selection" ||
                tree.0[2] == "fields_reference" && tree.1[0] == "Reference" ||
                tree.0[2] == "fields_relational" && tree.1[0] == "Many2one" ||
                tree.0[2] == "fields_reference" && tree.1[0] == "Many2oneReference" ||
                tree.0[2] == "fields_misc" && tree.1[0] == "Json" ||
                tree.0[2] == "fields_properties" && tree.1[0] == "Properties" ||
                tree.0[2] == "fields_properties" && tree.1[0] == "PropertiesDefinition" ||
                tree.0[2] == "fields_relational" && tree.1[0] == "One2many" ||
                tree.0[2] == "fields_relational" && tree.1[0] == "Many2many" ||
                tree.0[2] == "fields_misc" && tree.1[0] == "Id"
        ){
            return true;
        }
    } else {
        if tree.0.len() == 2 && tree.1.len() == 1 && tree.0[0] == "odoo" && tree.0[1] == "fields" {
            if matches!(tree.1[0].as_str(), "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
        "Binary" | "Image" | "Selection" | "Reference" | "Json" | "Properties" | "PropertiesDefinition" | "Id" | "Many2one" | "One2many" | "Many2many" | "Many2oneReference") {
                return true;
            }
        }
    }
    if is_inheriting_from_field(session, class_key) {
        return true;
    }
    false
}

pub fn is_field(session: &mut SessionInfo, target: SymbolKey) -> bool {
    let SymbolKey::Variable(v) = target else {
        return false;
    };
    let var_symbol = session.sync_odoo.symbol_table.variables.get(v).expect("valid key");
    let evaluations = var_symbol.evaluations.clone();
    for eval in evaluations {
        let symbol = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
        let eval_weaks = follow_ref(&symbol, session, &mut None, true, false, None, None);
        for eval_weak in eval_weaks.iter() {
            if let Some(key) = session.sync_odoo.symbol_table.upgrade_weak(eval_weak) {
                if is_field_class(session, key) {
                    return true;
                }
            }
        }
    }
    false
}

// @arena: helper for get_member_symbol
// @arena: code duplication with is_field
fn is_method(session: &mut SessionInfo, target: SymbolKey) -> bool {
    if matches!(target, SymbolKey::Function(_)) {
        return true;
    }
    let SymbolKey::Variable(v) = target else {
        return false;
    };
    let var_symbol = session.sync_odoo.symbol_table.variables.get(v).expect("valid key");
    let evals = var_symbol.evaluations.clone();
    for eval in evals.iter() {
        let symbol = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
        let eval_weaks = follow_ref(&symbol, session, &mut None, true, false, None, None);
        for eval_weak in eval_weaks.iter() {
            if let Some(key) = session.sync_odoo.symbol_table.upgrade_weak(eval_weak) {
                if matches!(key, SymbolKey::Function(_)) {
                    return true;
                }
            }
        }
    }
    false
}

/* Hook for get_member_symbol
Position is set to [0,0], because inside the method there is no concept of the current position.
The setting of the position is then delegated to the calling function.
TODO Consider refactoring.
    */
fn member_symbol_hook(session: &SessionInfo, target: SymbolKey, name: &String, diagnostics: &mut Vec<Diagnostic>){
    if session.sync_odoo.version_major >= 17 && name == "Form"{
        let tree = session.sync_odoo.symbol_table.get_tree(target);
        if tree.0.ends_with(&[Sy!("odoo"), Sy!("tests"), Sy!("common")]) && tree.1.is_empty() {
            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03301, &[]) {
                diagnostics.push(
                    Diagnostic {
                        range: Range::new(Position::new(0,0),Position::new(0,0)),
                        tags: Some(vec![DiagnosticTag::DEPRECATED]),
                        ..diagnostic_base.clone()
                    }
                );
            }
        }
    }
}

//store in result all available members for symbol: sub symbols, base class elements and models symbols
//TODO is order right of Vec in HashMap? if we take first or last in it, do we have the last effective value?
pub fn all_members(
    symbol: SymbolKey,
    session: &mut SessionInfo,
    with_co_models: bool,
    only_fields: bool,
    only_methods: bool,
    from_module: Option<SymbolKey>,
    is_super: bool
) -> HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>> {
    let mut result: HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>> = HashMap::new();
    let mut acc: HashSet<Tree> = HashSet::new();
    _all_members(symbol, session, &mut result, with_co_models, only_fields, only_methods, from_module, &mut acc, is_super);
    return  result;
}

fn _all_members(symbol_key: SymbolKey, session: &mut SessionInfo, result: &mut HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>>, with_co_models: bool, only_fields: bool, only_methods: bool, from_module: Option<SymbolKey>, acc: &mut HashSet<Tree>, is_super: bool) {
    macro_rules! st {
        () => { session.sync_odoo.symbol_table }
    }
    let tree = st!().get_tree(symbol_key);
    if acc.contains(&tree) {
        return;
    }
    acc.insert(tree);
    let mut append_result = |name: OYarn, symbol: SymbolKey, dep: Option<OYarn>| {
        if let Some(vec) = result.get_mut(&name) {
            vec.push((symbol, dep));
        } else {
            result.insert(name, vec![(symbol, dep)]);
        }
    };
    match symbol_key {
        SymbolKey::Class(c) => {
            // Skip current class symbols for super
            if !is_super{
                for symbol in get_sym!(st!(), symbol_key).all_symbols() {
                    if (only_fields && !is_field(session, symbol)) || (only_methods && !matches!(symbol, SymbolKey::Function(_))) {
                        continue;
                    }
                    let name = get_sym!(st!(), symbol).name().clone();
                    append_result(name, symbol, None);
                }
            }
            if with_co_models {
                let class_sym = st!().classes.get(c).expect("valid key");
                let Some(model) = class_sym._model.as_ref().and_then(|model_data|
                    session.sync_odoo.models.get(&model_data.name).cloned()
                ) else {
                    return;
                };
                // no recursion because it is handled in all_symbols_inherits
                let (model_symbols, model_inherits_symbols) = model.borrow().all_symbols_inherits(session, from_module);
                for (model_key, dependency) in model_symbols {
                    // @arena todo: adapt code below to knowing it's a class
                    let model_key = SymbolKey::from(model_key);
                    if dependency.is_some() || symbol_key == model_key {
                        continue;
                    }
                    let model_sym = get_sym!(st!(), model_key);
                    for s in model_sym.all_symbols() {
                        if (only_fields && !is_field(session, s)) || (only_methods && !matches!(s, SymbolKey::Function(_))) {
                            continue;
                        }
                        let name = get_sym!(st!(), s).name().clone();
                        let model_name = get_sym!(st!(), model_key).name().clone();
                        append_result(name, s, Some(model_name));
                    }
                }
                for (model_key, dependency) in model_inherits_symbols {
                    // @arena todo: adapt code below to knowing it's a class
                    let model_key = SymbolKey::from(model_key);
                    if dependency.is_some() || symbol_key == model_key {
                        continue;
                    }
                    let model_sym = get_sym!(st!(), model_key);
                    // for inherits symbols, we only add fields
                    let fields = model_sym.all_symbols().into_iter().filter(|&s| is_field(session, s)).collect::<Vec<_>>();
                    for s in fields {
                        let name = get_sym!(st!(), s).name().clone();
                        let model_name = get_sym!(st!(), model_key).name().clone();
                        append_result(name, s, Some(model_name));
                    }
                }
            }
            let bases = st!().classes[c].bases.iter()
                .filter_map(|base| base.upgrade(&st!()))
                .collect::<Vec<_>>();
            for base in bases {
                //no comodel as we will search for co-model from original class (what about overrided _name?)
                //TODO what about base of co-models classes?
                _all_members(base.into(), session, result, false, only_fields, only_methods, from_module, acc, false);
            }
        },
        SymbolKey::Function(_) => {
            // A function does not expose its symbols
        },
        // if not class just add it to result
        _ => {
            get_sym!(st!(), symbol_key).all_symbols().for_each(|s|
                if !(only_fields && !is_field(session, s)) {
                    let name = get_sym!(st!(), s).name().clone();
                    append_result(name, s, None);
                }
            )
        }
    }
}

pub fn all_fields(symbol: SymbolKey, session: &mut SessionInfo, from_module: Option<SymbolKey>) -> HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>> {
    all_members(symbol, session, true, true, false, from_module, false)
}

/* similar to get_symbol: will return the symbol that is under this one with the specified name.
However, if the symbol is a class or a model, it will search in the base class or in comodel classes
if not all, it will return the first found. If all, the all found symbols are returned, but the first one
is the one that is overriding others.
:param: from_module: optional, can change the from_module of the given class */
pub fn get_member_symbol(
    session: &mut SessionInfo,
    target: SymbolKey,
    name: &String,
    from_module: Option<SymbolKey>,
    prevent_comodel: bool,
    only_fields: bool,
    only_methods: bool,
    all: bool,
    is_super: bool
) -> (Vec<SymbolKey>, Vec<Diagnostic>) {
    let mut visited_classes: HashSet<ClassKey> = HashSet::new();
    return _get_member_symbol_helper(session, target, name, from_module, prevent_comodel, only_fields, only_methods, all, is_super, &mut visited_classes);
}

fn _get_member_symbol_helper(
    session: &mut SessionInfo,
    target: SymbolKey,
    name: &String,
    from_module: Option<SymbolKey>,
    prevent_comodel: bool,
    only_fields: bool,
    only_methods: bool,
    all: bool,
    is_super: bool,
    visited_classes: &mut HashSet<ClassKey>
) -> (Vec<SymbolKey>, Vec<Diagnostic>) {
    macro_rules! st {
        () => { session.sync_odoo.symbol_table }
    }
    let mut result: Vec<SymbolKey> = vec![];
    let mut visited_symbols: HashSet<SymbolKey> = HashSet::new();
    let extend_result = |syms: Vec<SymbolKey>, result: &mut Vec<SymbolKey>, visited_symbols: &mut HashSet<SymbolKey>| {
        syms.iter().for_each(|&sym|{
            if !visited_symbols.contains(&sym){
                visited_symbols.insert(sym);
                result.push(sym);
            }
        });
    };
    let mut diagnostics: Vec<Diagnostic> = vec![];
    member_symbol_hook(session, target, name, &mut diagnostics);
    let mod_sym = get_sym!(st!(), target).get_module_symbol(name);
    if let Some(mod_sym) = mod_sym {
        if !only_fields {
            if all {
                extend_result(vec![mod_sym], &mut result, &mut visited_symbols);
            } else {
                return (vec![mod_sym], diagnostics);
            }
        }
    }
    if !is_super {
        let mut content_syms = st!().get_sub_symbol(target, name, u32::MAX).symbols;
        if only_fields {
            content_syms = content_syms.iter().filter(|&&x| is_field(session, x)).copied().collect();
        }
        if only_methods {
            content_syms = content_syms.iter().filter(|&&x| is_method(session, x)).copied().collect();
        }
        if !content_syms.is_empty() {
            if all {
                extend_result(content_syms, &mut result, &mut visited_symbols);
            } else {
                return (content_syms, diagnostics);
            }
        }
    }
    let SymbolKey::Class(c) = target else {
        return (result, diagnostics);
    };
    let model_data = &st!().classes.get(c).expect("valid key")._model;
    if model_data.is_some() && !prevent_comodel {
        let model = session.sync_odoo.models.get(&model_data.as_ref().unwrap().name).cloned();
        if let Some(model) = model {
            let mut from_module = from_module.clone();
            if from_module.is_none() {
                from_module = st!().find_module(target);
            }
            if let Some(from_module) = from_module {
                let model_symbols = Model::get_full_model_symbols(model.clone(), session, from_module);
                for model_symbol in model_symbols {
                    if target == model_symbol.into() || visited_classes.contains(&model_symbol) {
                        continue;
                    }
                    visited_classes.insert(model_symbol);
                    let (attributs, att_diagnostic) = _get_member_symbol_helper(session, model_symbol.into(), name, None, true, only_fields, only_methods, all, false, visited_classes);
                    diagnostics.extend(att_diagnostic);
                    if all {
                        extend_result(attributs, &mut result, &mut visited_symbols);
                    } else {
                        if !attributs.is_empty() {
                            return (attributs, diagnostics);
                        }
                    }
                }
                for model_inherits_symbol in model.clone().borrow().get_inherits_models(session, from_module) {
                    //only fields are visible on inherits, not methods
                    let model_symbols = Model::get_full_model_symbols(model_inherits_symbol, session, from_module);
                    for model_symbol in model_symbols {
                        if target == model_symbol.into() || visited_classes.contains(&model_symbol) {
                            continue;
                        }
                        visited_classes.insert(model_symbol);
                        let (attributs, att_diagnostic) = _get_member_symbol_helper(session, model_symbol.into(), name, None, true, true, only_methods, all, false, visited_classes);
                        diagnostics.extend(att_diagnostic);
                        if all {
                            extend_result(attributs, &mut result, &mut visited_symbols);
                        } else {
                            if !attributs.is_empty() {
                                return (attributs, diagnostics);
                            }
                        }
                    }
                }
            }
        }
    }
    if result.is_empty() { // if we already have something, do not go up in bases
        let class_sym = &st!().classes[c];
        let bases = class_sym.bases.iter().filter_map(|w| w.upgrade(&st!())).collect::<Vec<_>>();
        for base in bases {
            if visited_classes.contains(&base){
                continue;
            }
            visited_classes.insert(base);
            let (s, s_diagnostic) = get_member_symbol(session, base.into(), name, from_module, prevent_comodel, only_fields, only_methods, all, false);
                diagnostics.extend(s_diagnostic);
            if !s.is_empty() {
                if all {
                    extend_result(s, &mut result, &mut visited_symbols);
                } else {
                    return (s, diagnostics);
                }
            }
        }
    }
    (result, diagnostics)
}


fn next_refs_class(session: &mut SessionInfo, class_key: ClassKey, context: &mut Option<Context>, symbol_context: &Context) -> Vec<EvaluationSymbolPtr> {
    macro_rules! st {
        () => { session.sync_odoo.symbol_table }
    }
    //if current symbol is a descriptor, we have to resolve __get__ method before going further
    let mut res = Vec::new();
    let mut base_attr = symbol_context.get("base_attr");
    if base_attr.is_none() {
        //search in context (used in decorators to indicate on which base the field is searched)
        if let Some(context) = context.as_ref() {
            base_attr = context.get("base_attr");
        }
    }
    let Some(base_attr) = base_attr else {
        return res;
    };
    let Some(base_attr) = base_attr.as_symbol().upgrade(&st!()) else {
        return res;
    };
    if !matches!(base_attr, SymbolKey::Class(_)) {
        return res;
    }
    //TODO shouldn't we set the from_module in the call to get_member_symbol?
    let get_method = get_member_symbol(session, class_key.into(), &S!("__get__"), None, true, false, false, true, false).0.first().copied();
    let Some(get_method) = get_method else {
        return res;
    };

    let get_sym_file = st!().get_file(get_method).unwrap();
    let get_method_sym = st!().get_symbol_view(get_method).expect("valid key from get_member_symbol");

    if get_method_sym.evaluations().is_some()
    && get_method_sym.evaluations().unwrap().len() == 0
    && !get_sym!(st!(), get_sym_file).is_external()
    && st!().build_status(get_sym_file, BuildSteps::ARCH_EVAL) == BuildStatus::DONE
    && st!().build_status(get_method, BuildSteps::ARCH) != BuildStatus::IN_PROGRESS
    && st!().build_status(get_method, BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS
    && st!().build_status(get_method, BuildSteps::VALIDATION) == BuildStatus::PENDING {
        let entry_point = st!().get_entry(get_method).unwrap();
        let mut v = PythonValidator::new(entry_point, get_method);
        v.validate(session);
    }
    let Some(evaluations) = get_sym!(st!(), get_method).evaluations().cloned() else {
        return res;
    };
    if context.is_none() {
        *context = Some(HashMap::new());
    }
    for get_method_eval in evaluations.iter() {
        context.as_mut().unwrap().extend(symbol_context.clone().into_iter());
        let get_result = get_method_eval.symbol.get_symbol_as_weak(session, context, &mut vec![], None);
        if !get_result.weak.is_expired(&st!()) {
            let mut eval = Evaluation::eval_from_symbol(&st!(), get_result.weak, get_result.instance);
            match eval.symbol.get_mut_symbol_ptr() {
                EvaluationSymbolPtr::WEAK(weak) => {
                    if let Some(eval_sym) = weak.weak.upgrade(&st!()) {
                        if eval_sym == class_key.into() {
                            continue;
                        }
                    }
                    weak.context.insert(S!("base_attr"), ContextValue::SYMBOL(base_attr.into()));
                    res.push(eval.symbol.get_symbol_ptr().clone());
                },
                _ => {}
            }
        }
        context.as_mut().unwrap().retain(|k, _| !symbol_context.contains_key(k));
    }
    res
}

fn next_refs_variable(session: &mut SessionInfo, key: VariableKey, context: &mut Option<Context>, symbol_context: &Context) -> Vec<EvaluationSymbolPtr> {
    let mut res = Vec::new();
    let var_symbol = session.sync_odoo.symbol_table.variables.get(key).expect("valid key");
    let evaluations = var_symbol.evaluations.clone();
    for eval in evaluations.iter() {
        let ctx = &mut Some(symbol_context.clone().into_iter().chain(context.clone().unwrap_or(HashMap::new()).into_iter()).collect::<HashMap<_, _>>());
        let mut sym = eval.symbol.get_symbol(session, ctx, &mut vec![], None);
        if let EvaluationSymbolPtr::WEAK(ref mut w) = sym {
            if let Some(base_attr) = symbol_context.get(&S!("base_attr")) {
                if !w.context.get(&S!("is_attr_of_instance")).map(|x| x.as_bool()).unwrap_or(false) {
                    w.context.insert(S!("base_attr"), base_attr.clone());
                }
            }
            if let Some(base_attr) = symbol_context.get(&S!("is_attr_of_instance")) {
                if !w.context.get(&S!("is_attr_of_instance")).map(|x| x.as_bool()).unwrap_or(false) {
                    w.context.insert(S!("is_attr_of_instance"), base_attr.clone());
                }
            }
        }
        if !session.sync_odoo.symbol_table.is_expired_if_weak(&sym) {
            res.push(sym);
        }
    }
    res
}

/*given a Symbol, give all the Symbol that are evaluated as valid evaluation for it.
example:
====
a = 5
if X:
    a = Test()
else:
    a = Object()
print(a)
====
next_refs on the 'a' in the print will return a SymbolRef to Test and one to Object
*/
fn next_refs(session: &mut SessionInfo, symbol_key: SymbolKey, context: &mut Option<Context>, symbol_context: &Context, stop_on_type: bool) -> Vec<EvaluationSymbolPtr> {
    match symbol_key {
        SymbolKey::Class(c) if !stop_on_type => next_refs_class(session, c, context, symbol_context),
        SymbolKey::Variable(v) => next_refs_variable(session, v, context, symbol_context),
        _ => vec![],
    }
}


/*
Follow evaluation of current symbol until type, value or end of the chain, depending or the parameters.
If a symbol in the chain is a descriptor, return the __get__ return evaluation.
If filter_on_tree is set, stop following when one of the symbols in the chain is in the tree, and only return those symbols.
    */
pub fn follow_ref(evaluation: &EvaluationSymbolPtr, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, filter_on_tree: Option<Tree>, max_scope: Option<SymbolKey>) -> Vec<EvaluationSymbolPtr> {
    macro_rules! st { () => { session.sync_odoo.symbol_table } }

    let default_result = match filter_on_tree.as_ref() {
        Some(_) => vec![],
        None => vec![evaluation.clone()],
    };
    let stop_on_tree_syms = filter_on_tree.map(|tree| session.sync_odoo.get_symbol("", &tree, u32::MAX));
    if matches!(stop_on_tree_syms.as_ref(), Some(syms) if syms.is_empty()) {
        // can't find the tree symbol, stop here
        return default_result;
    }
    let EvaluationSymbolPtr::WEAK(w) = evaluation else {
        // Non-weak evaluations are final
        return default_result
    };
    let Some(symbol_key) = w.weak.upgrade(&st!()) else {
        return default_result;
    };
    let symbol = get_sym!(st!(), symbol_key);
    if stop_on_value {
        if let Some(evals) = symbol.evaluations() {
            for eval in evals.iter() {
                if eval.value.is_some() {
                    return default_result;
                }
            }
        }
    }
    let can_eval_external = !symbol.is_external();
    //return a list of all possible evaluation: a weak ptr to the final symbol, and a bool indicating if this is an instance or not
    let mut work_queue: VecDeque<_> = next_refs(session, symbol_key, context, &w.context, stop_on_type).into_iter().collect();
    if work_queue.is_empty() {
        return default_result;
    }
    if w.instance.is_some_and(|v| v) {
        //if the previous evaluation was set to True, we want to keep it
        work_queue = work_queue.into_iter().map(|mut r| {
            if let EvaluationSymbolPtr::WEAK(ref mut weak) = r {
                weak.instance = Some(true);
            }
            r
        }).collect();
    }
    let mut results = Vec::new();
    let mut visited = HashSet::new();
    while let Some(current_eval) = work_queue.pop_front() {
        let EvaluationSymbolPtr::WEAK(next_ref_weak) = &current_eval else  {
            // Non-weak references are final
            results.push(current_eval);
            continue;
        };
        let Some(sym_key) = next_ref_weak.weak.upgrade(&st!()) else {
            // Discard evaluation to expired reference
            continue;
        };
        // Avoid cycles
        if visited.contains(&sym_key) {
            continue;
        }
        visited.insert(sym_key);
        let next_ref_weak_instance = next_ref_weak.instance.clone();
        match sym_key {
            SymbolKey::Variable(v) => {
                // let sym = sym_key.borrow();
                // let var = sym.as_variable();
                let var = &st!().variables[v];
                if (stop_on_type && matches!(next_ref_weak.is_instance(), Some(false)) && !var.is_import_variable) ||
                    (stop_on_value && var.evaluations.len() == 1 && var.evaluations[0].value.is_some()) ||
                    (max_scope.is_some() && !st!().has_in_parents(sym_key, max_scope.unwrap(), true)) {
                    // current evaluation is final
                    results.push(current_eval);
                    continue;
                }
                if let Some(stop_on_tree_syms) = stop_on_tree_syms.as_ref() {
                    if stop_on_tree_syms.iter().any(|s| *s == sym_key) {
                        results.push(current_eval);
                        continue;
                    }
                }
                if var.evaluations.is_empty() && var.name != "__all__" && can_eval_external {
                    //no evaluation? let's check that the file has been evaluated
                    let file_symbol = st!().get_file(sym_key);
                    if let Some(file_symbol) = file_symbol {
                        SyncOdoo::build_now(session, file_symbol, BuildSteps::ARCH_EVAL);
                    }
                }
                let mut next_sym_refs = next_refs_variable(session, v, context, &next_ref_weak.context);
                if next_sym_refs.is_empty() {
                    // keep current evaluation
                    results.push(current_eval);
                    continue;
                }
                // /!\ we want to keep instance = True if previous evaluation was set to True!
                if next_ref_weak_instance.is_some_and(|v| v) {
                    next_sym_refs = next_sym_refs.into_iter().map(|mut next_results| {
                        if let EvaluationSymbolPtr::WEAK(weak) = &mut next_results {
                            weak.instance = Some(true);
                        }
                        next_results
                    }).collect();
                }
                // enqueue evaluations to follow, replacing current evaluation
                work_queue.extend(next_sym_refs);
            },
            SymbolKey::Class(c) if !stop_on_type => {
                //On class, follow descriptor declarations
                let next_sym_refs = next_refs_class(session, c, context, &next_ref_weak.context);
                if next_sym_refs.is_empty() {
                    // keep current evaluation
                    results.push(current_eval);
                } else {
                    // enqueue evaluations to follow, replacing current evaluation
                    work_queue.extend(next_sym_refs);
                }
            },
            _ => {
                results.push(current_eval);
            }
        }
    }
    if let Some(stop_on_tree_syms) = stop_on_tree_syms.as_ref() {
        results.retain(|r| {
            match r {
                EvaluationSymbolPtr::WEAK(weak) => {
                    if let Some(key) = weak.weak.upgrade(&st!()) {
                        stop_on_tree_syms.iter().any(|&s| s == key)
                    } else {
                        false
                    }
                },
                _ => false
            }
        });
    }
    results
}

// @arena: might panic on last().unwrap()
pub fn is_specific_field_class(session: &SessionInfo, target: SymbolKey, field_names: &[&str]) -> bool {
    let tree = flatten_tree(&get_main_entry_tree(session, target));
    return is_field_class(session, target) && field_names.iter().any(|&name| {
        tree.last().unwrap() == name
    })
}

pub fn is_specific_field(session: &mut SessionInfo, target: SymbolKey, field_names: &[&str]) -> bool {
    macro_rules! st { () => { session.sync_odoo.symbol_table } }
    let SymbolKey::Variable(v) = target else {
        return false;
    };
    let evaluations = st!().variables.get(v).expect("valid key").evaluations.clone();
    for eval in evaluations.iter() {
        let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
        let eval_weaks = follow_ref(&symbol, session, &mut None, true, false, None, None);
        for eval_weak in eval_weaks.iter() {
            if let Some(symbol) = st!().upgrade_weak(eval_weak) {
                if is_specific_field_class(session, symbol, field_names){
                    return true;
                }
            }
        }
    }
    false
}

pub fn invalidate(session: &mut SessionInfo, symbol: SymbolKey, step: &BuildSteps) {
    macro_rules! st { () => { session.sync_odoo.symbol_table } }
    //signals that a change occurred to this symbol. "step" indicates which level of change occurred.
    //It will trigger rebuild on all dependencies
    let mut vec_to_invalidate = VecDeque::from([symbol]);
    while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
        let ref_to_inv_view = get_sym!(st!(), ref_to_inv);
        let sym_to_inv_type = ref_to_inv_view.typ();
        let dependents_len = ref_to_inv_view.dependents().len();
        if matches!(sym_to_inv_type, SymType::FILE | SymType::PACKAGE(_) | SymType::XML_FILE | SymType::CSV_FILE) {
            if *step == BuildSteps::ARCH && dependents_len > 0 {
                let sym_to_inv = get_sym!(st!(), ref_to_inv);
                let arch_dependents = &sym_to_inv.dependents()[BuildSteps::ARCH as usize];
                let mut build_queue = vec![];
                for (index, hashset) in arch_dependents.iter().enumerate() {
                    let Some(hashset) = hashset else {
                        continue;
                    };
                    for sym in hashset.iter_valid(|&k| st!().contains_key(k)) {
                        if !st!().is_symbol_in_parents(sym, ref_to_inv) {
                            build_queue.push((index, sym));
                        }
                    }
                }
                for (index, sym) in build_queue {
                    if index == BuildSteps::ARCH as usize {
                        session.sync_odoo.add_to_rebuild_arch(sym);
                    } else if index == BuildSteps::ARCH_EVAL as usize {
                        session.sync_odoo.add_to_rebuild_arch_eval(sym);
                    } else if index == BuildSteps::VALIDATION as usize {
                        st!().invalidate_sub_functions(sym);
                        session.sync_odoo.add_to_validations(sym);
                    }
                }
            }
            if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(step) && dependents_len > 1 {
                let sym_to_inv = get_sym!(st!(), ref_to_inv);
                let arch_eval_dependents = &sym_to_inv.dependents()[BuildSteps::ARCH_EVAL as usize];
                let mut build_queue = vec![];
                for (index, hashset) in arch_eval_dependents.iter().enumerate() {
                    let Some(hashset) = hashset else {
                        continue;
                    };
                    for sym in hashset.iter_valid(|&k| st!().contains_key(k)) {
                        if !st!().is_symbol_in_parents(sym, ref_to_inv) {
                            build_queue.push((index, sym));
                        }
                    }
                }
                for (index, sym) in build_queue {
                    if index + 1 == BuildSteps::ARCH_EVAL as usize {
                        session.sync_odoo.add_to_rebuild_arch_eval(sym);
                    } else if index + 1 == BuildSteps::VALIDATION as usize {
                        st!().invalidate_sub_functions(sym);
                        session.sync_odoo.add_to_validations(sym);
                    }
                }
                for class in st!().iter_classes(ref_to_inv) {
                    let class_sym = st!().classes.get(class).expect("valid key");
                    if let Some(model_data) = &class_sym._model {
                        let model = session.sync_odoo.models.get(&model_data.name).cloned();
                        if let Some(model) = model {
                            let from_module = st!().find_module(class);
                            // @arena: todo: check if this mutates the dependents of sym_to_inv
                            model.borrow().add_dependents_to_validation(session, from_module);
                        }
                    }
                }
            }
        }
        if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::VALIDATION].contains(step) && dependents_len > 2 {
            let sym_to_inv = get_sym!(st!(), ref_to_inv);
            let validation_dependents = &sym_to_inv.dependents()[BuildSteps::VALIDATION as usize];
            for sym in validation_dependents.iter().flatten()
                    .flat_map(|s| s.iter_valid(|&k| st!().contains_key(k)))
                    .collect::<Vec<_>>() {
                if !st!().is_symbol_in_parents(sym, ref_to_inv) {
                    st!().invalidate_sub_functions(sym);
                    session.sync_odoo.add_to_validations(sym);
                }
            }
        }
        let sym_to_inv = get_sym!(st!(), ref_to_inv);
        if sym_to_inv.has_modules() {
            for &sym in sym_to_inv.all_module_symbol() {
                vec_to_invalidate.push_back(sym);
            }
        }
    }
}


/**
 * @arena
 * consider creating a new type for storing as weak in evaluation.
 * upgrade weak would convert it to symbol type.
 * This avoids forgetting checking if key is valid every time
 * table_symbol.get_valid_key(weak_key) -> Option(SymbolKey)
 * use it also where PtrWeakHashSets were used. This forces the caller
 * iterating on it to upgrade them
 */

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

impl SymbolTable {
    pub fn upgrade(&self, weak_key: Weak<SymbolKey>) -> Option<SymbolKey> {
        weak_key.upgrade(self)
    }

    pub fn get_from_weak(&self, weak_key: Weak<SymbolKey>) -> Option<SymbolView> {
        if let Some(key) = weak_key.upgrade(self) {
            self.get_symbol_view(key)
        } else {
            None
        }
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

pub trait ContainsKey<K> {
    fn contains_key(&self, key: K) -> bool;
}

impl ContainsKey<ClassKey> for SymbolTable {
    fn contains_key(&self, key: ClassKey) -> bool {
        self.classes.contains_key(key)
    }
}

impl ContainsKey<PackageKey> for SymbolTable {
    fn contains_key(&self, key: PackageKey) -> bool {
        self.packages.contains_key(key)
    }
}

impl ContainsKey<SymbolKey> for SymbolTable {
    fn contains_key(&self, key: SymbolKey) -> bool {
        match key {
            SymbolKey::Root(k) => self.roots.contains_key(k),
            SymbolKey::DiskDir(k) => self.disk_dirs.contains_key(k),
            SymbolKey::Namespace(k) => self.namespaces.contains_key(k),
            SymbolKey::Package(k) => self.packages.contains_key(k),
            SymbolKey::File(k) => self.files.contains_key(k),
            SymbolKey::Compiled(k) => self.compiled.contains_key(k),
            SymbolKey::Class(k) => self.classes.contains_key(k),
            SymbolKey::Function(k) => self.functions.contains_key(k),
            SymbolKey::Variable(k) => self.variables.contains_key(k),
            SymbolKey::XmlFile(k) => self.xml_files.contains_key(k),
            SymbolKey::CsvFile(k) => self.csv_files.contains_key(k),
        }
    }
}

//  implement  also a Strong<> variant. Slotmap operations with a Strong would panic (with expect message), and
//  the programmer would skip the check.
