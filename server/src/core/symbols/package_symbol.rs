use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::{dependency_mgr::{Buildable, Dependencies}, symbol_table::SymbolKey}, xml_data::OdooData}, oyarn, threads::SessionInfo, weak_hash_set::WeakSet};
use std::{cell::RefCell, collections::{HashMap, HashSet}, path::PathBuf, rc::Weak};

use super::{module_symbol::ModuleSymbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub enum PackageSymbol {
    PythonPackage(PythonPackageSymbol),
    Module(ModuleSymbol)
}

impl PackageSymbol {
    pub fn new_python_package(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        PackageSymbol::PythonPackage(PythonPackageSymbol::new(name, path, parent, is_external))
    }
    pub fn new_module_package(session: &mut SessionInfo, name: &str, path: &PathBuf, parent:SymbolKey, is_external: bool) -> Option<Self> {
        if let Some(module) = ModuleSymbol::new(session, name, path, parent, is_external) {
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
    pub fn parent(&self) -> SymbolKey {
        match self {
            PackageSymbol::Module(m) => m.parent,
            PackageSymbol::PythonPackage(p) => p.parent,
        }
    }

    pub fn i_ext(&self) -> &'static str {
        match self {
            PackageSymbol::Module(m) => m.i_ext,
            PackageSymbol::PythonPackage(p) => p.i_ext,
        }
    }
    pub fn set_i_ext(&mut self, ext: &'static str) {
        match self {
            PackageSymbol::PythonPackage(p) => {p.i_ext = ext},
            PackageSymbol::Module(m) => {m.i_ext = ext},
        }
    }
    // @arena dead code
    // pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
    //     match self {
    //         PackageSymbol::Module(m) => m.dependencies(),
    //         PackageSymbol::PythonPackage(p) => &p.dependencies()
    //     }
    // }
    pub fn dependencies_as_mut(&mut self) -> &mut Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match self {
            PackageSymbol::Module(m) => &mut m.dependencies,
            PackageSymbol::PythonPackage(p) => &mut p.dependencies,
        }
    }
    pub fn dependents(&self) -> &Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match self {
            PackageSymbol::Module(m) => m.dependents(),
            PackageSymbol::PythonPackage(p) => &p.dependents()
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut Vec<Vec<Option<WeakSet<SymbolKey>>>> {
        match self {
            PackageSymbol::Module(m) => &mut m.dependents,
            PackageSymbol::PythonPackage(p) => &mut p.dependents,
        }
    }
    pub fn add_file(&mut self, file: SymbolKey, name: &str) {
        match self {
            PackageSymbol::Module(m) => m.module_symbols.insert(oyarn!("{}", name), file),
            PackageSymbol::PythonPackage(p) => p.module_symbols.insert(oyarn!("{}", name), file),
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

    pub fn in_workspace(&self) -> bool {
        match self {
            PackageSymbol::Module(m) => m.in_workspace,
            PackageSymbol::PythonPackage(p) => p.in_workspace,
        }
    }

    pub fn as_module_package_mut(&mut self) -> &mut ModuleSymbol {
        match self {
            PackageSymbol::Module(m) => m,
            _ => panic!("Not a module package"),
        }
    }

    pub fn as_module_package(&self) -> &ModuleSymbol {
        match self {
            PackageSymbol::Module(m) => m,
            _ => panic!("Not a module package"),
        }
    }

    pub fn as_python_package_mut(&mut self) -> &mut PythonPackageSymbol {
        match self {
            PackageSymbol::PythonPackage(p) => p,
            _ => panic!("Not a python package"),
        }
    }
}

impl Buildable for PackageSymbol {
    fn build_status(&self, step: BuildSteps) -> BuildStatus {
        match self {
            PackageSymbol::Module(m) => m.build_status(step),
            PackageSymbol::PythonPackage(p) => p.build_status(step),
        }
    }
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        match self {
            PackageSymbol::Module(m) => m.set_build_status(step, status),
            PackageSymbol::PythonPackage(p) => p.set_build_status(step, status),
        }
    }
}

#[derive(Debug)]
pub struct PythonPackageSymbol {
    pub name: OYarn,
    pub path: String,
    pub i_ext: &'static str,
    pub is_external: bool,
    pub parent: SymbolKey,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub in_workspace: bool,
    pub self_import: bool,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>, //used for dynamic XML_ID records, like ir.models
    pub module_symbols: HashMap<OYarn, SymbolKey>,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<WeakSet<SymbolKey>>>>,
    pub dependents: Vec<Vec<Option<WeakSet<SymbolKey>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>,

    // @arena: dead code?
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, HashSet<SymbolKey>>,
    pub decl_ext_symbols: HashMap<SymbolKey, HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>>
}

impl PythonPackageSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            i_ext: "",
            parent,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            xml_ids: HashMap::new(),
            self_import: false, //indicates that if unloaded, the symbol should be added in the rebuild automatically as nothing depends on it (used for root packages)
            module_symbols: HashMap::new(),
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            decl_ext_symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res._init_symbol_mgr();
        res
    }

    // @arena: moved to SymbolMgr
    // pub fn add_symbol(&mut self, content: SymbolKey, name: &OYarn, section: u32) {
    //     let sections = self.symbols.entry(name.clone()).or_insert(HashMap::new());
    //     let section_vec = sections.entry(section).or_insert(vec![]);
    //     section_vec.push(content);
    // }

    // @arena: moved to SymbolTable
    // pub fn get_ext_symbol(&self, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
    //     let mut result = vec![];
    //     if let Some(owners) = self.ext_symbols.get(name) {
    //         for owner in owners.iter() {
    //             let owner = owner.borrow();
    //             result.extend(owner.get_decl_ext_symbol(&self.weak_self.as_ref().unwrap().upgrade().unwrap(), name));
    //         }
    //     }
    //     result
    // }

    // @arena: moved to SymbolView
    // pub fn get_decl_ext_symbol(&self, symbol: &Rc<RefCell<Symbol>>, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
    //     let mut result = vec![];
    //     if let Some(object_decl_symbols) = self.decl_ext_symbols.get(symbol) {
    //         if let Some(symbols) = object_decl_symbols.get(name) {
    //             for end_symbols in symbols.values() {
    //                 //TODO actually we don't take position into account, but can we really?
    //                 result.extend(end_symbols.iter().map(|s| s.clone()));
    //             }
    //         }
    //     }
    //     result
    // }

}
