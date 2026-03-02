use std::{cell::RefCell, cmp::Ordering, collections::{HashMap, HashSet, hash_map}, path::PathBuf, rc::Rc};

use ruff_python_ast::ExprCall;
use ruff_text_size::TextRange;
use slotmap::{SlotMap, new_key_type};
use tracing::trace;

use crate::{constants::{BuildStatus, BuildSteps, OYarn, PackageType, SymType, Tree, flatten_tree}, core::{entry_point::EntryPoint, evaluation::Evaluation, model::Model, symbols::{
    class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol, dependency_mgr::{Buildable, Dependencies}, disk_dir_symbol::DiskDirSymbol, ext_symbol_store::ExtSymbolStore, file_symbol::FileSymbol, function_symbol::{Argument, FunctionSymbol}, module_symbol::ModuleSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, root_symbol::RootSymbol, symbol, symbol_mgr::{ContentSymbols, SectionIndex, SectionRange, SymbolMgr, iter_symbol_keys}, variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol
}}, threads::SessionInfo, utils::{PathSanitizer, compare_semver}};

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

    // @arena: original code did not rely on dyn dispatch (as_symbol_mgr)
    pub fn iter_symbols(&self) -> hash_map::Iter<'_, OYarn, HashMap<u32, Vec<SymbolKey>>> {
        self.as_symbol_mgr().get_symbols().iter()
    }

    // @arena: like the original, this is not lazy iteration (might as well just return the Vec)
    pub fn all_symbols(&self) -> impl Iterator<Item = SymbolKey> {
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

    pub fn get_symbol_first_path(&self) -> String{
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

    pub fn dependents(&self) -> &Vec<Vec<Option<HashSet<SymbolKey>>>> {
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
    
    pub fn get_all_dependencies(&self, step: BuildSteps) -> Option<&Vec<Option<HashSet<SymbolKey>>>> {
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

    pub fn as_symbol_mgr(&self) -> &dyn SymbolMgr {
        match self {
            Self::File(f) => *f,
            Self::Class(c) => *c,
            Self::Function(f) => *f,
            Self::Package(PackageSymbol::Module(m)) => m,
            Self::Package(PackageSymbol::PythonPackage(p)) => p,
            _ => {panic!("Not a symbol Mgr");}
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

    pub fn contains_key(&self, key: SymbolKey) -> bool {
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
        if symbol == to_test {
            return true;
        }
        let symbol_view = self.get_symbol_view(symbol).expect("valid key");
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
    pub fn find_module(&self, key: SymbolKey) -> Option<SymbolKey> {
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
        let target_sym = self.get_symbol_view(target).expect("valid key");
        let target_sym_mgr = target_sym.as_symbol_mgr();
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
        let target_sym = self.get_symbol_view(target).expect("valid key");
        let target_sym_mgr = target_sym.as_symbol_mgr();
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
        let file_sym = self.get_symbol_view(file).expect("valid key"); // formely Rc (strong)
        let file_sym_mgr = file_sym.as_symbol_mgr();
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
    fn dependencies_mut(&mut self, valid_key: SymbolKey) -> &mut Vec<Vec<Option<HashSet<SymbolKey>>>> {
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

    fn dependents_as_mut(&mut self, valid_key: SymbolKey) -> &mut Vec<Vec<Option<HashSet<SymbolKey>>>> {
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
            .get_or_insert_with(HashSet::new)
            .insert(dependency);

        // register `target` as a dependent of `depends_on`
        self.dependents_as_mut(dependency)[level_i][step_i - level_i]
            .get_or_insert_with(HashSet::new)
            .insert(target);
    }

    /* Helper to merge dependencies eval_from_ast will fill when called. To be called on a file/package... */
    pub fn insert_dependencies(&mut self, target: SymbolKey, deps: &mut Vec<Vec<SymbolKey>>, current_step: BuildSteps) {
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

pub fn is_inheriting_from_field(session: &SessionInfo, symbol_key: SymbolKey) -> bool {
    // if not class return false
    if !matches!(symbol_key, SymbolKey::Class(_)) {
        return false;
    }
    let tree = flatten_tree(&get_main_entry_tree(session, symbol_key));
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
    let symbol = symbol_table.get_symbol_view(symbol_key).expect("valid key");
    // @arena: ClassSymbol.bases are weak refs
    for &base_key in symbol.as_class_sym().bases.iter().filter(|&&k| symbol_table.contains_key(k)) {
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
    let result = is_field_class_uncached(session, symbol_key);
    cache.borrow_mut().replace(result);
    result
}

fn is_field_class_uncached(session: &SessionInfo, symbol_key: SymbolKey) -> bool {
    let tree = &get_main_entry_tree(session, symbol_key);
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
    if is_inheriting_from_field(session, symbol_key) {
        return true;
    }
    false
}
