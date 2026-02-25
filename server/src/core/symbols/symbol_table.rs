use std::{cell::RefCell, path::PathBuf, rc::Rc};

use ruff_text_size::TextRange;
use slotmap::{SlotMap, new_key_type};

use crate::{constants::{OYarn, PackageType, SymType, Tree, tree}, core::{entry_point::EntryPoint, file_mgr::FileMgr, symbols::{
    class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol, disk_dir_symbol::DiskDirSymbol, ext_symbol_store::ExtSymbolStore, file_symbol::FileSymbol, function_symbol::FunctionSymbol, module_symbol::ModuleSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, root_symbol::RootSymbol, symbol, symbol_mgr::SymbolMgr, variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol
}}, threads::SessionInfo, utils::PathSanitizer};

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



    // @arena get_symbol + unwrap is the equivalent of upgrade + unwrap on a weak ref
    // @arena, to check possibly weird things:
    // - different behavior for root before and inside the loop
    // - loop stops if symbol has no parent, without including it.
    pub fn get_tree(&self, symbol_key: SymbolKey) -> Tree {
        let symbol = self.get_symbol(symbol_key).expect("valid key");
        let mut res = (vec![], vec![]);
        if symbol.is_file_content() {
            res.1.insert(0, symbol.name().clone());
        } else {
            res.0.insert(0, symbol.name().clone());
        }
        if symbol.typ() == SymType::ROOT || symbol.parent().is_none() {
            return res
        }
        let mut current_key = symbol.parent().unwrap();
        let mut current_sym = self.get_symbol(current_key).expect("valid key");
        while current_sym.typ() != SymType::ROOT && current_sym.parent().is_some() {
            if current_sym.is_file_content() {
                res.1.insert(0, current_sym.name().clone());
            } else {
                res.0.insert(0, current_sym.name().clone());
            }
            current_key = current_sym.parent().unwrap();
            current_sym = self.get_symbol(current_key).expect("valid key");
        }
        res
    }

    // @arena
    // formerly a method on Symbol, so valid target expected
    // original code unwrapped the upgrade() of weak without checking
    pub fn get_in_parents(&self, target: SymbolKey, sym_types: &[SymType], stop_same_file: bool) -> Option<SymbolKey> {
        let target_symbol = self.get_symbol(target).expect("valid key");
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
            .and_then(|root_key| self.get_symbol(root_key))
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

    // Formerly called like self.find_module on a Symbol after borrowing the Rc/RefCell
    // Now called directly with the key
    // @arena: compare with get_in_parents, and chose an approach (trust the key or not)
    // Consider just calling get_in_parents
    pub fn find_module(&self, key: SymbolKey) -> Option<SymbolKey> {
        let symbol = self.get_symbol(key)?;
        if let SymbolView::Package(PackageSymbol::Module(_)) = symbol {
            return Some(key);
        }
        return self.find_module(symbol.parent()?);
    }

// TODO then: add_new_xml_file, add_new_csv_file

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

// @arena: associated function is SymbolTable?
///Given a path, create the appropriated symbol and attach it to the given parent
pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: SymbolKey, require_module: bool) -> Option<SymbolKey> {
    let symbol_table = &mut session.sync_odoo.symbol_table;
    let name: String = if path.is_dir() {
        path.components().last().unwrap().as_os_str().to_str().unwrap().to_string()
    } else {
        path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string()
    };
    let path_str = path.sanitize();
    if path_str.ends_with(".py") || path_str.ends_with(".pyi") || FileMgr::is_untitled(&path_str) {
        return Some(symbol_table.add_new_file(parent, &name, &path_str));
    }
    let main_entry_tree = get_main_entry_tree(session, parent);
    if main_entry_tree == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
        let module = SymbolTable::add_new_module_package(session, parent, &name, path);
        let symbol_table = &mut session.sync_odoo.symbol_table;
        if let Some(module) = module {
            let module_symbol = symbol_table.get_symbol(module).unwrap();
            let dir_name = module_symbol.as_module_package().dir_name.clone();
            session.sync_odoo.modules.insert(dir_name, module);
            return Some(module);
        } else if require_module {
            return None;
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let package_key = symbol_table.add_new_python_package(parent, &name, &path_str);
                if !path.join("__init__.py").exists() {
                    symbol_table.packages.get_mut(package_key).unwrap().set_i_ext("i");
                }
                return Some(package_key.into());
            } else {
                return None;
            }
        }
    } else if require_module {
        return None;
    } else {
        let symbol_table = &mut session.sync_odoo.symbol_table;
        if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
            if main_entry_tree == tree(vec!["odoo"], vec![]) && path_str.ends_with("addons") {
                //Force namespace for odoo/addons
                let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
                return Some(namespace_key);
            } else {
                // let ref_sym = parent.borrow_mut().add_new_python_package(session, &name, &path_str);
                let package_key = symbol_table.add_new_python_package(parent, &name, &path_str);
                if !path.join("__init__.py").exists() {
                    symbol_table.packages.get_mut(package_key).unwrap().set_i_ext("i");
                }
                return Some(package_key.into());
            }
        } else if path.is_dir() {
            let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
            return Some(namespace_key);
        }
    }
    None
}