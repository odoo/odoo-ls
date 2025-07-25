use weak_table::{PtrWeakHashSet, PtrWeakKeyHashMap};

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, xml_data::XmlData}, oyarn, threads::SessionInfo, S};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::{Rc, Weak}};

use super::{module_symbol::ModuleSymbol, symbol::Symbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub enum PackageSymbol {
    PythonPackage(PythonPackageSymbol),
    Module(ModuleSymbol)
}

impl PackageSymbol {
    pub fn new_python_package(name: String, path: String, is_external: bool) -> Self {
        PackageSymbol::PythonPackage(PythonPackageSymbol::new(name, path, is_external))
    }
    pub fn new_module_package(session: &mut SessionInfo, name: String, path: &PathBuf, is_external: bool) -> Option<Self> {
        if let Some(module) = ModuleSymbol::new(session, name, path, is_external) {
            Some(PackageSymbol::Module(module))
        } else {
            None
        }
    }
    pub fn name(&self) -> &OYarn {
        match self {
            PackageSymbol::PythonPackage(p) => &p.name,
            PackageSymbol::Module(m) => &m.name,
        }
    }
    pub fn parent(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            PackageSymbol::Module(m) => m.parent.clone(),
            PackageSymbol::PythonPackage(p) => p.parent.clone()
        }
    }
    pub fn set_parent(&mut self, parent: Option<Weak<RefCell<Symbol>>>) {
        match self {
            PackageSymbol::Module(m) => m.parent = parent,
            PackageSymbol::PythonPackage(p) => p.parent = parent,
        }
    }
    pub fn i_ext(&self) -> &String {
        match self {
            PackageSymbol::Module(m) => &m.i_ext,
            PackageSymbol::PythonPackage(p) => &p.i_ext,
        }
    }
    pub fn set_i_ext(&mut self, ext: String) {
        match self {
            PackageSymbol::PythonPackage(p) => {p.i_ext = ext},
            PackageSymbol::Module(m) => {m.i_ext = ext},
        }
    }
    pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            PackageSymbol::Module(m) => m.dependencies(),
            PackageSymbol::PythonPackage(p) => &p.dependencies()
        }
    }
    pub fn dependencies_as_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            PackageSymbol::Module(m) => m.dependencies_mut(),
            PackageSymbol::PythonPackage(p) => p.dependencies_mut()
        }
    }
    pub fn dependents(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            PackageSymbol::Module(m) => m.dependents(),
            PackageSymbol::PythonPackage(p) => &p.dependents()
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            PackageSymbol::Module(m) => m.dependents_mut(),
            PackageSymbol::PythonPackage(p) => p.dependents_mut()
        }
    }
    pub fn add_file(&mut self, file: &Rc<RefCell<Symbol>>) {
        match self {
            PackageSymbol::Module(m) => m.module_symbols.insert(file.borrow().name().clone(), file.clone()),
            PackageSymbol::PythonPackage(p) => p.module_symbols.insert(file.borrow().name().clone(), file.clone()),
        };
    }
    pub fn paths(&self) -> Vec<String> {
        match self {
            PackageSymbol::Module(m) => vec![m.path.clone()],
            PackageSymbol::PythonPackage(p) => vec![p.path.clone()],
        }
    }
    pub fn is_external(&self) -> bool {
        match self {
            PackageSymbol::Module(m) => m.is_external,
            PackageSymbol::PythonPackage(p) => p.is_external,
        }
    }
}

#[derive(Debug)]
pub struct PythonPackageSymbol {
    pub name: OYarn,
    pub path: String,
    pub i_ext: String,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub in_workspace: bool,
    pub self_import: bool,
    pub xml_ids: HashMap<OYarn, Vec<XmlData>>, //used for dynamic XML_ID records, like ir.models
    pub module_symbols: HashMap<OYarn, Rc<RefCell<Symbol>>>,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub dependents: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, PtrWeakHashSet<Weak<RefCell<Symbol>>>>,
    pub decl_ext_symbols: PtrWeakKeyHashMap<Weak<RefCell<Symbol>>, HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>>
}

impl PythonPackageSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            path,
            is_external,
            i_ext: S!(""),
            weak_self: None,
            parent: None,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            xml_ids: HashMap::new(),
            self_import: false, //indicates that if unloaded, the symbol should be added in the rebuild automatically as nothing depends on it (used for root packages)
            module_symbols: HashMap::new(),
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            decl_ext_symbols: PtrWeakKeyHashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res._init_symbol_mgr();
        res
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert(HashMap::new());
        let section_vec = sections.entry(section).or_insert(vec![]);
        section_vec.push(content.clone());
    }

    pub fn get_dependencies(&self, step: usize, level: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependencies.get(step)?.get(level)?.as_ref()
    }

    pub fn get_all_dependencies(&self, step: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependencies.get(step)
    }

    pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependencies
    }

    pub fn dependencies_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependencies
    }
    pub fn get_dependents(&self, level: usize, step: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependents.get(level)?.get(step)?.as_ref()
    }

    pub fn get_all_dependents(&self, level: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependents.get(level)
    }

    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        self.in_workspace = in_workspace;
        if in_workspace {
            self.dependencies= vec![
                vec![ //ARCH
                    None //ARCH
                ],
                vec![ //ARCH_EVAL
                    None, //ARCH,
                    None, //ARCH_EVAL
                ],
                vec![
                    None, // ARCH
                    None, //ARCH_EVAL
                    None, //VALIDATIOn
                ]
            ];
            self.dependents = vec![
                vec![ //ARCH
                    None, //ARCH
                    None, //ARCH_EVAL
                    None, //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    None, //ARCH_EVAL
                    None //VALIDATION
                ],
                vec![ //VALIDATION
                    None //VALIDATION
                ]
            ];
        }
    }

    pub fn dependents(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependents
    }

    pub fn dependents_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependents
    }

    pub fn is_in_workspace(&self) -> bool {
        self.in_workspace
    }

    pub fn get_ext_symbol(&self, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result = vec![];
        if let Some(owners) = self.ext_symbols.get(name) {
            for owner in owners.iter() {
                let owner = owner.borrow();
                result.extend(owner.get_decl_ext_symbol(&self.weak_self.as_ref().unwrap().upgrade().unwrap(), name));
            }
        }
        result
    }

    pub fn get_decl_ext_symbol(&self, symbol: &Rc<RefCell<Symbol>>, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result = vec![];
        if let Some(object_decl_symbols) = self.decl_ext_symbols.get(symbol) {
            if let Some(symbols) = object_decl_symbols.get(name) {
                for end_symbols in symbols.values() {
                    //TODO actually we don't take position into account, but can we really?
                    result.extend(end_symbols.iter().map(|s| s.clone()));
                }
            }
        }
        result
    }

}