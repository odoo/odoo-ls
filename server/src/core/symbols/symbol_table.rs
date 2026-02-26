use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

use ruff_python_ast::ExprCall;
use ruff_text_size::TextRange;
use slotmap::{SlotMap, new_key_type};

use crate::{constants::{OYarn, PackageType, SymType, Tree}, core::{entry_point::EntryPoint, symbols::{
    class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol, disk_dir_symbol::DiskDirSymbol, ext_symbol_store::ExtSymbolStore, file_symbol::FileSymbol, function_symbol::{Argument, FunctionSymbol}, module_symbol::ModuleSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PackageSymbol, root_symbol::RootSymbol, symbol_mgr::{ContentSymbols, SectionIndex, SectionRange, SymbolMgr}, variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol
}}, threads::SessionInfo};

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

    ///return true if to_test is in parents of symbol or equal to it.
    /// @arena: originally Rc's
    pub fn is_symbol_in_parents(&self, symbol: SymbolKey, to_test: SymbolKey) -> bool {
        if symbol == to_test {
            return true;
        }
        let symbol_view = self.get_symbol(symbol).expect("valid key");
        let Some(parent) = symbol_view.parent() else {
            return false;
        };
        self.is_symbol_in_parents(parent, to_test)
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
        let target_sym = self.get_symbol(target).expect("valid key");
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
    fn _get_loc_symbol(&self, target: &dyn SymbolMgr, map: &HashMap<u32, Vec<SymbolKey>>, position: u32, index: &SectionIndex, acc: &mut HashSet<u32>) -> ContentSymbols {
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
                        let loc_sym = self.get_symbol(sym_key).expect("valid key");
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
        let target_sym = self.get_symbol(target).expect("valid key");
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

    // @arena: possible undeflow bug/panic: subttractions followed by cast to u32
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
                let arg_sym = self.get_symbol(arg.symbol).expect("valid key");
                if *arg_sym.name() == keyword.arg.as_ref().unwrap().id {
                    return Some(arg);
                }
            }
        } else {
            return func.args.get(arg_index as usize);
        }
        None
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
