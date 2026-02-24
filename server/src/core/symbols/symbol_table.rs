use std::{collections::{HashMap, HashSet}, path::PathBuf};

use ruff_text_size::TextRange;
use slotmap::{SlotMap, new_key_type};

use crate::{constants::{OYarn, PackageType, SymType, Tree, tree}, core::{file_mgr::FileMgr, symbols::{
    class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol, disk_dir_symbol::DiskDirSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol, module_symbol::ModuleSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, root_symbol::RootSymbol, symbol, symbol_mgr::SymbolMgr, variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol
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
}

/// section index → [variable keys]                                                                  
type SectionSymbols = HashMap<u32, Vec<SymbolKey>>;                                                  
/// name → section symbols
type NamedSectionSymbols = HashMap<OYarn, SectionSymbols>;                                           
/// target/host → named section symbols
type DeclExtSymbols = HashMap<SymbolKey, NamedSectionSymbols>;
/// name → set of owner keys
type ExtSymbolOwners = HashMap<OYarn, HashSet<SymbolKey>>;

#[derive(Debug)]
pub struct ExtSymbolStore {
    /// formerly ext_symbols
    /// target → name → owners
    pub(crate) owners: HashMap<SymbolKey, ExtSymbolOwners>,
    /// formerly decl_ext_symbols
    /// owner → target → name → section → [variable keys]
    declarations: HashMap<SymbolKey, DeclExtSymbols>,
}

impl ExtSymbolStore {
    fn new() -> Self {
        Self {
            owners: HashMap::new(),
            declarations: HashMap::new(),
        }
    }

    fn add(&mut self, target: SymbolKey, owner: SymbolKey, name: OYarn, section: u32, variable: SymbolKey) {
        self.owners
            .entry(target).or_default()
            .entry(name.clone()).or_default()
            .insert(owner);

        self.declarations
            .entry(owner).or_default()
            .entry(target).or_default()
            .entry(name).or_default()
            .entry(section).or_default()
            .push(variable);
    }

    // @arena: former get_decl_ext_symbol
    // Gets the symbol (`name`) injected by `owner` into `target`
    fn get(&self, owner: SymbolKey, target: SymbolKey, name: &OYarn) -> Vec<SymbolKey> {
        let Some(decl_ext_symbols ) = self.declarations.get(&owner) else {
            return vec![];
        };
        let mut result = vec![];
        if let Some(object_decl_symbols) = decl_ext_symbols.get(&target) {
            if let Some(symbols) = object_decl_symbols.get(name) {
                for end_symbols in symbols.values() {
                    //TODO actually we don't take position into account, but can we really?
                    result.extend(end_symbols);
                }
            }
        }
        result
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
    ext_symbols: ExtSymbolStore,
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
    pub fn add_new_python_package(&mut self, parent: SymbolKey, name: &str, path: &str) -> PackageKey {
        let is_external = self.parent_is_external(parent);
        let package_symbol = PackageSymbol::new_python_package(name, path, parent, is_external);
        let package_key = self.packages.insert(package_symbol);
        self.register_in_parent(parent, package_key.into(), name, path);
        package_key
    }

    pub fn add_new_namespace(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.register_in_parent(parent, namespace_key.into(), name, path);
        namespace_key.into()
    }

    pub fn add_new_disk_dir(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.register_in_parent(parent, disk_dir_key.into(), name, path);
        disk_dir_key.into()
    }

    pub fn add_new_compiled(&mut self, parent: SymbolKey, name: &str, path: &str) -> SymbolKey {
        let is_external = self.parent_is_external(parent);
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key: SymbolKey = self.compiled.insert(compiled_symbol).into();
        match parent {
            SymbolKey::Namespace(n) => {
                self.namespaces.get_mut(n).unwrap().add_file(compiled_key, name, path);
            },
            SymbolKey::Package(p) => {
                self.packages.get_mut(p).unwrap().add_file(compiled_key, name);
            },
            SymbolKey::Root(r) => {
                self.roots.get_mut(r).unwrap().add_file(compiled_key, name);
            },
            SymbolKey::Compiled(c) => {
                self.compiled.get_mut(c).unwrap().add_compiled(compiled_key, name);
            }
            SymbolKey::DiskDir(d) => {
                self.disk_dirs.get_mut(d).unwrap().add_file(compiled_key, name);
            },
            _ => {
                panic!("Impossible to add a {} to a {}", 
                    self.get_symbol(compiled_key).unwrap().typ(), 
                    self.get_symbol(parent).unwrap().typ()
                );
            }
        }
        compiled_key
    }

    // @arena: not a method! (takes SessionInfo as arg)
    pub fn add_new_module_package(session: &mut SessionInfo, parent: SymbolKey, name: &str, path: &PathBuf) -> Option<SymbolKey> {
        let is_external = session.sync_odoo.symbol_table.parent_is_external(parent);
        let module = PackageSymbol::new_module_package(session, name, path, parent, is_external)?;
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

    // ==== external symbols =====

    // @arena: This used to be a method in each Symbol variant
    pub fn get_ext_symbol(&self, target: SymbolKey, name: &OYarn) -> Vec<SymbolKey> {
        let Some(ext_symbols) = self.ext_symbols.owners.get(&target) else {
            return vec![];
        };

        let mut result = vec![];
        if let Some(owners) = ext_symbols.get(name) {
            for &owner in owners {
                if !self.contains_key(owner) {
                    // @arena: Equivalent of iterating on a PtrWeakHashSet, which cleans up expired weaks
                    // todo: remove key from ExtSymbolStore
                    continue;
                }
                result.extend(self.ext_symbols.get(owner, target, name));
            }
        }
        result
    }

    // @arena: assumes owner as valid key (formerly an strong Rc)
    // @arena TODO: fix this weird API (take &str instead of OYarn)
    pub fn add_new_ext_symbol(
        &mut self,
        target: SymbolKey,
        name: OYarn,
        range: &TextRange,
        owner: SymbolKey,
    ) -> SymbolKey {
        let target_sym = self.get_symbol(target).expect("valid key");
        // validate target can host an external symbol
        if !matches!(target_sym.typ(),
            SymType::FILE | SymType::PACKAGE(PackageType::MODULE)
                | SymType::PACKAGE(PackageType::PYTHON_PACKAGE)
                | SymType::CLASS | SymType::FUNCTION | SymType::NAMESPACE
        ) {
            panic!("Impossible to add an external symbol to a {}", target_sym.typ());
        }
        let variable_symbol = VariableSymbol::new(
            name.clone(),
            target,
            range.clone(),
            target_sym.is_external(),
        );
        let variable_key: SymbolKey = self.variables.insert(variable_symbol).into();
        let section = self.get_section_for_key(owner, range.start().to_u32());

        self.ext_symbols.add(target, owner, name, section, variable_key);
        variable_key
    }
    
    // @arena: assumes owner as valid key (formerly self on a Symbol)
    /* used by add_new_ext_symbol. Do not call directly */
    fn get_section_for_key(&self, owner: SymbolKey, position: u32) -> u32 {
        match owner {
            SymbolKey::File(f) => self.files[f].get_section_for(position).index,
            SymbolKey::Package(p) => {
                match &self.packages[p] {
                    PackageSymbol::Module(m) => m.get_section_for(position).index,
                    PackageSymbol::PythonPackage(p) => p.get_section_for(position).index,
                }
            },
            SymbolKey::Class(c) => self.classes[c].get_section_for(position).index,
            SymbolKey::Function(f) => self.functions[f].get_section_for(position).index,
            _ => panic!(
                "Impossible to add a declaration of external symbol to a {}",
                self.get_symbol(owner).unwrap().typ()
            ),
        }
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