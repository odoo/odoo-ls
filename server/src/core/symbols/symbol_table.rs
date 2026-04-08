use std::{cell::RefCell, cmp::Ordering, collections::{HashMap, HashSet, VecDeque, hash_map}, ops::{Index, IndexMut}, path::PathBuf, rc::Rc};

use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::ExprCall;
use ruff_text_size::{TextRange, TextSize};
use slotmap::{Key, SlotMap, new_key_type};
use tracing::{info, trace};

use crate::{S, Sy, constants::{BuildStatus, BuildSteps, DEBUG_MEMORY, OYarn, PackageType, SymType, Tree, flatten_tree}, core::{diagnostics::{DiagnosticCode, create_diagnostic}, entry_point::EntryPoint, evaluation::{Context, ContextValue,
Evaluation, EvaluationSymbolPtr}, file_mgr::NoqaInfo, model::Model, odoo::SyncOdoo, python_validator::PythonValidator, symbols::{ class_symbol::ClassSymbol, compiled_symbol::CompiledSymbol, csv_file_symbol::CsvFileSymbol, dependency_mgr::{Buildable, Dependencies}, disk_dir_symbol::DiskDirSymbol, ext_symbol_store::ExtSymbolStore, file_symbol::FileSymbol, function_symbol::{Argument, FunctionSymbol}, module_symbol::ModuleSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PythonPackageSymbol, root_symbol::RootSymbol, symbol_keys::{ClassKey, CompiledKey, ContainsKey, CsvFileKey, DiskDirKey, FileKey, FunctionKey, ModuleKey, NamespaceKey, PythonPackageKey, RootKey, SymbolKey, VariableKey, Weak, XmlFileKey}, symbol_mgr::{ContentSymbols, SectionIndex, SectionRange, SymbolMgr, iter_symbol_keys}, variable_symbol::VariableSymbol, xml_file_symbol::XmlFileSymbol }, xml_data::OdooData}, oyarn, threads::SessionInfo, utils::{PathSanitizer, compare_semver}, weak_hash_set::WeakSet};

#[derive(Debug)]
pub enum SymbolView<'a> {
    Root(&'a RootSymbol),
    DiskDir(&'a DiskDirSymbol),
    Namespace(&'a NamespaceSymbol),
    PythonPackage(&'a PythonPackageSymbol),
    Module(&'a ModuleSymbol),
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
    // @arena deprecated
    pub fn parent(&self) -> Option<SymbolKey> {
        match self {
            Self::Root(_) => None,
            Self::DiskDir(s) => Some(s.parent()),
            Self::Namespace(s) => Some(s.parent()),
            Self::PythonPackage(s) => Some(s.parent()),
            Self::Module(s) => Some(s.parent()),
            Self::File(s) => Some(s.parent()),
            Self::Compiled(s) => Some(s.parent()),
            Self::Class(s) => Some(s.parent()),
            Self::Function(s) => Some(s.parent()),
            Self::Variable(s) => Some(s.parent()),
            Self::XmlFileSymbol(s) => Some(s.parent()),
            Self::CsvFileSymbol(s) => Some(s.parent()),
        }
    }

    // @arena deprecated
    pub fn is_external(&self) -> bool {
        match self {
            Self::Root(_) => false,
            Self::DiskDir(d) => d.is_external,
            Self::Namespace(n) => n.is_external,
            Self::PythonPackage(p) => p.is_external,
            Self::Module(p) => p.is_external,
            Self::File(f) => f.is_external,
            Self::Compiled(c) => c.is_external,
            Self::Class(c) => c.is_external,
            Self::Function(f) => f.is_external,
            Self::Variable(v) => v.is_external,
            Self::XmlFileSymbol(x) => x.is_external,
            Self::CsvFileSymbol(c) => c.is_external,
        }
    }

    // @arena deprecated
    pub fn name(&self) -> &OYarn {
        match self {
            Self::Root(s) => &s.name,
            Self::DiskDir(s) => &s.name,
            Self::Namespace(s) => &s.name,
            Self::PythonPackage(p) => &p.name,
            Self::Module(p) => &p.name,
            Self::File(f) => &f.name,
            Self::Compiled(c) => &c.name,
            Self::Class(c) => &c.name,
            Self::Function(f) => &f.name,
            Self::Variable(v) => &v.name,
            Self::XmlFileSymbol(x) => &x.name,
            Self::CsvFileSymbol(c) => &c.name,
        }
    }

    // @arena: deprecated
    pub fn in_workspace(&self) -> bool {
        match self {
            Self::Root(_) => false,
            Self::Namespace(n) => n.is_in_workspace(),
            Self::DiskDir(d) => d.in_workspace,
            Self::Module(m) => m.in_workspace,
            Self::PythonPackage(p) => p.in_workspace,
            Self::File(f) => f.is_in_workspace(),
            Self::Compiled(_) => panic!(),
            Self::Class(_) => panic!(),
            Self::Function(_) => panic!(),
            Self::Variable(_) => panic!(),
            Self::XmlFileSymbol(x) => x.is_in_workspace(),
            Self::CsvFileSymbol(c) => c.is_in_workspace(),
        }
    }

    // @arena: depreacated
    pub fn paths(&self) -> Vec<String> {
        match self {
            Self::Root(_) => vec![],
            Self::Namespace(n) => n.paths(),
            Self::DiskDir(d) => vec![d.path.clone()],
            Self::PythonPackage(p) => vec![p.path.clone()],
            Self::Module(m) => vec![m.path.clone()],
            Self::File(f) => vec![f.path.clone()],
            Self::Compiled(c) => vec![c.path.clone()],
            Self::Class(_) => vec![],
            Self::Function(_) => vec![],
            Self::Variable(_) => vec![],
            Self::XmlFileSymbol(x) => vec![x.path.clone()],
            Self::CsvFileSymbol(c) => vec![c.path.clone()],
        }
    }

    pub fn dependents(&self) -> &Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match self {
            Self::Root(_) => panic!("No dependencies on Root"),
            Self::Namespace(n) => n.dependents(),
            Self::DiskDir(_) => panic!("No dependencies on DiskDir"),
            Self::PythonPackage(p) => p.dependents(),
            Self::Module(m) => m.dependents(),
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
            Self::Module(m) => m.get_all_dependencies(step as usize),
            Self::PythonPackage(p) => p.get_all_dependencies(step as usize),
            Self::File(f) => f.get_all_dependencies(step as usize),
            Self::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Self::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Self::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Self::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
            Self::XmlFileSymbol(x) => x.get_all_dependencies(step as usize),
            Self::CsvFileSymbol(c) => c.get_all_dependencies(step as usize),
        }
    }
}


#[derive(Debug)]
pub struct SymbolTable {
    // slotmaps per symbol type
    roots: SlotMap<RootKey, RootSymbol>,
    disk_dirs: SlotMap<DiskDirKey, DiskDirSymbol>,
    namespaces: SlotMap<NamespaceKey, NamespaceSymbol>,
    python_packages: SlotMap<PythonPackageKey, PythonPackageSymbol>,
    modules: SlotMap<ModuleKey, ModuleSymbol>,
    files: SlotMap<FileKey, FileSymbol>,
    compiled: SlotMap<CompiledKey, CompiledSymbol>,
    classes: SlotMap<ClassKey, ClassSymbol>,
    functions: SlotMap<FunctionKey, FunctionSymbol>,
    variables: SlotMap<VariableKey, VariableSymbol>,
    xml_files: SlotMap<XmlFileKey, XmlFileSymbol>,
    csv_files: SlotMap<CsvFileKey, CsvFileSymbol>,
    // external symbols
    pub(super) ext_symbols: ExtSymbolStore,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            roots: SlotMap::with_key(),
            disk_dirs: SlotMap::with_key(),
            namespaces: SlotMap::with_key(),
            python_packages: SlotMap::with_key(),
            modules: SlotMap::with_key(),
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
    
    // @arena: temp (remove me)
    pub fn get_symbol_view(&self, key: SymbolKey) -> Option<SymbolView<'_>> {
        match key {
            SymbolKey::Root(k) => self.roots.get(k).map(SymbolView::Root),
            SymbolKey::DiskDir(k) => self.disk_dirs.get(k).map(SymbolView::DiskDir),
            SymbolKey::Namespace(k) => self.namespaces.get(k).map(SymbolView::Namespace),
            SymbolKey::PythonPackage(k) => self.python_packages.get(k).map(SymbolView::PythonPackage),
            SymbolKey::Module(k) => self.modules.get(k).map(SymbolView::Module),
            SymbolKey::File(k) => self.files.get(k).map(SymbolView::File),
            SymbolKey::Compiled(k) => self.compiled.get(k).map(SymbolView::Compiled),
            SymbolKey::Class(k) => self.classes.get(k).map(SymbolView::Class),
            SymbolKey::Function(k) => self.functions.get(k).map(SymbolView::Function),
            SymbolKey::Variable(k) => self.variables.get(k).map(SymbolView::Variable),
            SymbolKey::XmlFile(k) => self.xml_files.get(k).map(SymbolView::XmlFileSymbol),
            SymbolKey::CsvFile(k) => self.csv_files.get(k).map(SymbolView::CsvFileSymbol),
        }
    }
    
    /// Symbol creation and destruction.
    ///
    /// All slotmap insertions/removals and parent/child relationship mutations are
    /// centralized here. The `symbols` and `module_symbols` fields on variant structs
    /// are `pub(super)`, so only code within `core::symbols` can mutate them.
    /// Combined with private slotmaps, this guarantees that `parent`, `symbols`, and
    /// `module_symbols` always hold valid keys — they can be trusted without validity
    /// checks, unlike keys stored elsewhere (e.g. in dependency sets or evaluations).
    
    pub fn new_root(&mut self) -> RootKey {
        let root_symbol = RootSymbol::new();
        self.roots.insert(root_symbol)
    }
    // @arena: parent is a verified existing key
    // Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, parent: SymbolKey, name: &str, path: &str) -> FileKey {
        let is_external = self.parent_is_external(parent);
        let file_symbol = FileSymbol::new(name, path, parent, is_external);
        let file_key = self.files.insert(file_symbol);
        self.register_in_parent(parent, file_key.into(), name, path);
        file_key
    }

    // @arena: parent is a verified existing key - Consider adding a validate_key method
    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, parent: SymbolKey, name: &str, path: &str, i_ext: &'static str) -> PythonPackageKey {
        let is_external = self.parent_is_external(parent);
        let package_symbol = PythonPackageSymbol::new(name, path, parent, is_external, i_ext);
        let package_key = self.python_packages.insert(package_symbol);
        self.register_in_parent(parent, package_key.into(), name, path);
        package_key
    }

    pub fn add_new_namespace(&mut self, parent: SymbolKey, name: &str, path: &str) -> NamespaceKey {
        let is_external = self.parent_is_external(parent);
        let namespace_symbol = NamespaceSymbol::new(name, vec![path.to_string()], parent, is_external);
        let namespace_key = self.namespaces.insert(namespace_symbol);
        self.register_in_parent(parent, namespace_key.into(), name, path);
        namespace_key
    }

    pub fn add_new_disk_dir(&mut self, parent: SymbolKey, name: &str, path: &str) -> DiskDirKey {
        let is_external = self.parent_is_external(parent);
        let disk_dir_symbol = DiskDirSymbol::new(name, path, parent, is_external);
        let disk_dir_key = self.disk_dirs.insert(disk_dir_symbol);
        self.register_in_parent(parent, disk_dir_key.into(), name, path);
        disk_dir_key
    }

    pub fn add_new_compiled(&mut self, parent: SymbolKey, name: &str, path: &str) -> CompiledKey {
        let is_external = self.parent_is_external(parent);
        let compiled_symbol = CompiledSymbol::new(name, path, parent, is_external);
        let compiled_key = self.compiled.insert(compiled_symbol);
        match parent {
            SymbolKey::Compiled(c) => {
                self.compiled[c].module_symbols.insert(oyarn!("{}", name), compiled_key.into());
            },
            _ => {
                self.register_in_parent(parent, compiled_key.into(), name, path);
            }
        }
        compiled_key
    }

    // @arena: not a method! (takes SessionInfo as arg)
    pub fn add_new_module_package(session: &mut SessionInfo, parent: SymbolKey, name: &str, path: &PathBuf) -> Option<ModuleKey> {
        let is_external = session.sync_odoo.symbol_table.parent_is_external(parent);
        let module = ModuleSymbol::new(session, name, path, parent, is_external)?;
        let symbol_table = &mut session.sync_odoo.symbol_table;
        let module_key = symbol_table.modules.insert(module);
        symbol_table.register_in_parent(parent, module_key.into(), name, &path.sanitize());
        Some(module_key)
    }

    // ====== Helpers for symbol creation ======

    // @arena: this would be simpler if is_external returned true for root
    fn parent_is_external(&self, parent: SymbolKey) -> bool {
        match parent {
            SymbolKey::Root(_) => true,
            _ => self.is_external(parent),
        }
    }

    fn register_in_parent(&mut self, parent: SymbolKey, child: SymbolKey, name: &str, path: &str) {
        match parent {
            SymbolKey::Namespace(n) => {
                self.add_file_to_namespace(n, child, name, path);
            },
            SymbolKey::PythonPackage(p) => {
                self.python_packages[p].module_symbols.insert(oyarn!("{}", name), child);
            },
            SymbolKey::Module(m) => {
                self.modules[m].module_symbols.insert(oyarn!("{}", name), child);
            },
            SymbolKey::Root(r) => {
                self.roots[r].module_symbols.insert(oyarn!("{}", name), child);
            },
            SymbolKey::DiskDir(d) => {
                self.disk_dirs[d].module_symbols.insert(oyarn!("{}", name), child);
            },
            _ => {
                panic!("Impossible to add a {} to a {}", child.typ(), parent.typ());
            }
        }
    }
    
    fn add_file_to_namespace(&mut self, parent: NamespaceKey, file: SymbolKey, name: &str, path: &str) {
        let ns = &mut self.namespaces[parent];
        let best = ns.directories.iter()
            .enumerate()
            .filter(|(_, dir)| PathBuf::from(path).starts_with(&dir.path))
            .max_by_key(|(_, dir)| dir.path.len())
            .unwrap_or_else(|| panic!("Not valid path found to add the file ({}) to namespace {} with directories {:?}", path, ns.name, ns.directories))
            .0;
        ns.directories[best].module_symbols.insert(oyarn!("{}", name), file);
    }

    // @arena: consider taking &str for name
    pub fn add_new_variable(&mut self, parent: impl Into<SymbolKey>, name: OYarn, range: &TextRange) -> VariableKey {
        let parent = parent.into();
        let is_external = self.is_external(parent);
        let variable_symbol = VariableSymbol::new(name.clone(), parent, range.clone(), is_external);
        let variable_key = self.variables.insert(variable_symbol);
        self.add_to_parent_symbols(parent, variable_key.into(), &name, range.start().to_u32());
        variable_key
    }
    pub fn add_new_function(&mut self, parent: SymbolKey, name: &str, range: &TextRange, body_start: &TextSize) -> FunctionKey {
        let is_external = self.is_external(parent);
        let function_symbol = FunctionSymbol::new(name, parent, range.clone(), body_start.clone(), is_external);
        let function_key = self.functions.insert(function_symbol);
        self.add_to_parent_symbols(parent, function_key.into(), name, range.start().to_u32());
        function_key
    }

    pub fn add_new_class(&mut self, parent: SymbolKey, name: &String, range: &TextRange, body_start: &TextSize) -> ClassKey {
        let is_external = self.is_external(parent);
        let class_symbol = ClassSymbol::new(name, parent, range.clone(), body_start.clone(), is_external);
        let class_key = self.classes.insert(class_symbol);
        self.add_to_parent_symbols(parent, class_key.into(), name, range.start().to_u32());
        class_key.into()
    }

    fn add_to_parent_symbols(&mut self, parent: SymbolKey, content: SymbolKey, name: &str, position: u32) {
         match parent {
            SymbolKey::File(f) => {
                let file = &mut self.files[f];
                let section = file.get_section_for(position).index;
                file.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::Module(m) => {
                let module = &mut self.modules[m];
                let section = module.get_section_for(position).index;
                module.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::PythonPackage(p) => {
                let package = &mut self.python_packages[p];
                let section = package.get_section_for(position).index;
                package.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::Class(c) => {
                let class = &mut self.classes[c];
                let section = class.get_section_for(position).index;
                class.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            },
            SymbolKey::Function(f) => {
                let function = &mut self.functions[f];
                let section = function.get_section_for(position).index;
                function.symbols.entry(oyarn!("{}",name)).or_default()
                    .entry(section).or_default()
                    .push(content);
            }
            _ => {
                panic!("Impossible to add a {} to a {}", content.typ(), parent.typ());
            }
        }
    }


    pub fn add_new_xml_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> XmlFileKey {
        let parent_symbol = self.modules.get(parent).expect("valid key");
        let mut xml_file_symbol = XmlFileSymbol::new(name, path, parent.into(), parent_symbol.is_external);
        xml_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let xml_file_key = self.xml_files.insert(xml_file_symbol);
        self.register_data_file(parent, path, xml_file_key.into());
        xml_file_key
    }

    /// parent is a module package
    pub fn add_new_csv_file(&mut self, parent: ModuleKey, name: &str, path: &str) -> CsvFileKey {
        let parent_symbol = self.modules.get(parent).expect("valid key");
        let mut csv_file_symbol = CsvFileSymbol::new(name, path, parent.into(), parent_symbol.is_external);
        csv_file_symbol.set_in_workspace(parent_symbol.in_workspace);
        let csv_file_key = self.csv_files.insert(csv_file_symbol);
        self.register_data_file(parent, path, csv_file_key.into());
        csv_file_key
    }

    /// parent is a module package
    fn register_data_file(&mut self, parent: ModuleKey, path: &str, data_file: SymbolKey) {
        let entry = self.get_entry(parent.into()).unwrap();
        entry.borrow_mut().data_symbols.insert(path.to_string(), data_file.into());

        self.modules[parent].data_symbols.insert(path.to_string(), data_file);
    }

    // @arena: this is not done in the original code.
    // unload_path does it, apparently
    fn unregister_data_file(&mut self, parent: ModuleKey, path: &str) {
        let entry = self.get_entry(parent.into()).unwrap();
        entry.borrow_mut().data_symbols.remove(path);

        self.modules[parent].data_symbols.remove(path);
    }
    
    // @arena: assumes owner as valid key (formerly a strong Rc)
    pub fn add_new_ext_symbol(
        &mut self,
        target: SymbolKey,
        name: &str,
        range: &TextRange,
        owner: SymbolKey,
    ) -> SymbolKey {
        // validate target can host an external symbol
        if !matches!(target.typ(),
            SymType::FILE | SymType::PACKAGE(PackageType::MODULE)
                | SymType::PACKAGE(PackageType::PYTHON_PACKAGE)
                | SymType::CLASS | SymType::FUNCTION | SymType::NAMESPACE
        ) {
            panic!("Impossible to add an external symbol to a {}", target.typ());
        }
        let name = oyarn!("{}", name);
        let variable_symbol = VariableSymbol::new(
            name.clone(),
            target,
            range.clone(),
            self.is_external(target),
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
            SymbolKey::File(f) => self[f].get_section_for(position).index,
            SymbolKey::Module(m) => self[m].get_section_for(position).index,
            SymbolKey::PythonPackage(p) => self[p].get_section_for(position).index,
            SymbolKey::Class(c) => self[c].get_section_for(position).index,
            SymbolKey::Function(f) => self[f].get_section_for(position).index,
            _ => panic!("Impossible to add a declaration of external symbol to a {}", owner.typ()),
        }
    }
    
    fn remove(&mut self, key: SymbolKey) {
        self.ext_symbols.remove(key);
        match key {
            SymbolKey::Root(k) => { self.roots.remove(k); }
            SymbolKey::DiskDir(k) => { self.disk_dirs.remove(k); }
            SymbolKey::Namespace(k) => { self.namespaces.remove(k); }
            SymbolKey::PythonPackage(k) => { self.python_packages.remove(k); }
            SymbolKey::Module(k) => { self.modules.remove(k); }
            SymbolKey::File(k) => { self.files.remove(k); }
            SymbolKey::Compiled(k) => { self.compiled.remove(k); }
            SymbolKey::Class(k) => { self.classes.remove(k); }
            SymbolKey::Function(k) => { self.functions.remove(k); }
            SymbolKey::Variable(k) => { self.variables.remove(k); }
            SymbolKey::XmlFile(k) => { self.xml_files.remove(k); }
            SymbolKey::CsvFile(k) => { self.csv_files.remove(k); }
        }
    }

    // @arena: removes a symbol from its parent (not yet from the symbol table)
    // original code in unload + remove symbol: unwraps Option(parent) and the weak.upgrade.
    fn remove_symbol(&mut self, child: SymbolKey) {
        let child_symbol = self.get_symbol_view(child).expect("valid key");
        let child_name = child_symbol.name().clone();
        let parent = child_symbol.parent().expect("symbol should have a parent");
        if self.is_file_content(child) {
            match parent {
                SymbolKey::Class(c) => { self.classes[c].symbols.remove(&child_name); },
                SymbolKey::File(f) => { self.files[f].symbols.remove(&child_name); },
                SymbolKey::Function(f) => { self.functions[f].symbols.remove(&child_name); },
                SymbolKey::Module(m) => { self.modules[m].symbols.remove(&child_name); },
                SymbolKey::PythonPackage(p) => { self.python_packages[p].symbols.remove(&child_name); },
                SymbolKey::DiskDir(_) => { panic!("A disk directory can not contain python code") },
                SymbolKey::Compiled(_) => { panic!("A compiled symbol can not contain python code") },
                SymbolKey::Namespace(_) => { panic!("A namespace can not contain python code") },
                SymbolKey::Root(_) => { panic!("Root can not contain python code") },
                SymbolKey::Variable(_) => { panic!("A variable can not contain python code") }
                SymbolKey::XmlFile(_) => { panic!("An XML file symbol can not contain python code") }
                SymbolKey::CsvFile(_) => { panic!("A CSV file symbol can not contain python code") }
            };
        } else {
            match parent {
                SymbolKey::Class(_) => { panic!("A class can not contain a file structure") },
                SymbolKey::File(_) => { panic!("A file can not contain a file structure"); },
                SymbolKey::Function(_) => { panic!("A function can not contain a file structure") },
                SymbolKey::DiskDir(d) => { self.disk_dirs[d].module_symbols.remove(&child_name); },
                SymbolKey::Module(m) => match child {
                    SymbolKey::XmlFile(x) => {
                        self.unregister_data_file(m, &self.xml_files[x].path.clone());
                    },
                    SymbolKey::CsvFile(c) => {
                        self.unregister_data_file(m, &self.csv_files[c].path.clone());
                    },
                    _ => {
                        self.modules[m].module_symbols.remove(&child_name);
                    },
                },
                SymbolKey::PythonPackage(p) => {
                    self.python_packages[p].module_symbols.remove(&child_name);
                },
                SymbolKey::Compiled(c) => { self.compiled[c].module_symbols.remove(&child_name); },
                SymbolKey::Namespace(n) => {
                    for directory in self.namespaces[n].directories.iter_mut() {
                        directory.module_symbols.remove(&child_name);
                    }
                },
                SymbolKey::Root(r) => { self.roots[r].module_symbols.remove(&child_name); },
                SymbolKey::Variable(_) => { panic!("A variable can not contain a file structure"); }
                SymbolKey::XmlFile(_) => { panic!("An XML file symbol can not contain a file structure") }
                SymbolKey::CsvFile(_) => { panic!("A CSV file symbol can not contain a file structure") }
            };
        }
    } 
    
    //unload a symbol and subsymbols.
    // @arena:  removes the entry from the symbol table
    // remove_symbol only removes the symbol from its parent, and some extra clean up in case
    // of data files. Consider moving the clean up to here. Or to split the remove
    // from parent + cleanup in separate functions per key type, that would mirror the add_* methods.
    pub fn unload(session: &mut SessionInfo, symbol: SymbolKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        let mut vec_to_unload = VecDeque::from([symbol]);
        while !vec_to_unload.is_empty() {
            let ref_to_unload = *vec_to_unload.front().unwrap();
            let sym_ref = get_sym!(st!(), ref_to_unload);
            // Unload children first
            let mut found_one = false;
            for sym in st!().all_symbols(ref_to_unload) {
                found_one = true;
                vec_to_unload.push_front(sym);
            }
            if found_one {
                continue;
            }
            vec_to_unload.pop_front();
            if DEBUG_MEMORY && matches!(ref_to_unload.typ(), SymType::FILE  | SymType::PACKAGE(_)) {
                info!("Unloading symbol {:?} at {:?}", st!().name(ref_to_unload), st!().paths(ref_to_unload));
            }
            let module = st!().find_module(ref_to_unload);
            //unload symbol
            let parent = *sym_ref.parent().as_ref().unwrap();
            st!().remove_symbol(ref_to_unload);
            if matches!(ref_to_unload, SymbolKey::File(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) | SymbolKey::XmlFile(_) | SymbolKey::CsvFile(_)) {
                Self::invalidate(session, ref_to_unload, &BuildSteps::ARCH);
            }
            //check if we should not reimport automatically
            match ref_to_unload {
                SymbolKey::PythonPackage(p) => {
                    let package = &st!().python_packages[p];
                    if package.self_import {
                        session.sync_odoo.must_reload_paths.push((parent.into(), package.path.clone()));
                    }
                }
                SymbolKey::File(f) => {
                    let file = &st!().files[f];
                    if file.self_import {
                        session.sync_odoo.must_reload_paths.push((parent.into(), file.path.clone()));
                    }
                }
                _ => {}
            }
            match ref_to_unload {
                SymbolKey::Module(p) => {
                    let m = &st!().modules[p];
                    // @arena: because of this, values in sync_odoo.modules can be trusted (make it not a Weak then?)
                    session.sync_odoo.modules.remove(m.dir_name.as_str());
                }
                SymbolKey::Class(c) => {
                    let class = &st!().classes[c];
                    if let Some(model_data) = class._model.as_ref() {
                        let model = session.sync_odoo.models.get(&model_data.name).cloned();
                        if let Some(model) = model {
                            model.borrow_mut().remove_symbol(session, c,  module);
                        }
                    }
                }
                _ => {}
            }
            st!().remove(ref_to_unload);
        }
    }
    
    
    pub fn pre_allocate(&mut self) {
        self.files.reserve(7000);
        // self.packages.reserve(2200);
        self.classes.reserve(14000);
        self.functions.reserve(80000);
        self.variables.reserve(450000);
        self.xml_files.reserve(3200);
    }
}


impl ContainsKey<ClassKey> for SymbolTable {
    fn contains_key(&self, key: ClassKey) -> bool {
        self.classes.contains_key(key)
    }
}

impl ContainsKey<PythonPackageKey> for SymbolTable {
    fn contains_key(&self, key: PythonPackageKey) -> bool {
        self.python_packages.contains_key(key)
    }
}

impl ContainsKey<ModuleKey> for SymbolTable {
    fn contains_key(&self, key: ModuleKey) -> bool {
        self.modules.contains_key(key)
    }
}

impl ContainsKey<SymbolKey> for SymbolTable {
    fn contains_key(&self, key: SymbolKey) -> bool {
        match key {
            SymbolKey::Root(k) => self.roots.contains_key(k),
            SymbolKey::DiskDir(k) => self.disk_dirs.contains_key(k),
            SymbolKey::Namespace(k) => self.namespaces.contains_key(k),
            SymbolKey::PythonPackage(k) => self.python_packages.contains_key(k),
            SymbolKey::Module(k) => self.modules.contains_key(k),
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

macro_rules! impl_index {
    ($key:ty, $output:ty, $field:ident) => {
        impl Index<$key> for SymbolTable {
            type Output = $output;
            fn index(&self, key: $key) -> &$output {
                &self.$field[key]
            }
        }
        impl IndexMut<$key> for SymbolTable {
            fn index_mut(&mut self, key: $key) -> &mut $output {
                &mut self.$field[key]
            }
        }
    };
}

impl_index!(RootKey, RootSymbol, roots);
impl_index!(DiskDirKey, DiskDirSymbol, disk_dirs);
impl_index!(NamespaceKey, NamespaceSymbol, namespaces);
impl_index!(PythonPackageKey, PythonPackageSymbol, python_packages);
impl_index!(ModuleKey, ModuleSymbol, modules);
impl_index!(FileKey, FileSymbol, files);
impl_index!(CompiledKey, CompiledSymbol, compiled);
impl_index!(FunctionKey, FunctionSymbol, functions);
impl_index!(ClassKey, ClassSymbol, classes);
impl_index!(VariableKey, VariableSymbol, variables);
impl_index!(XmlFileKey, XmlFileSymbol, xml_files);
impl_index!(CsvFileKey, CsvFileSymbol, csv_files);

