use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps}, threads::SessionInfo, S};
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
    pub fn name(&self) -> &String {
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
    pub fn dependencies(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4] {
        match self {
            PackageSymbol::Module(m) => &m.dependencies,
            PackageSymbol::PythonPackage(p) => &p.dependencies
        }
    }
    pub fn dependencies_as_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4] {
        match self {
            PackageSymbol::Module(m) => &mut m.dependencies,
            PackageSymbol::PythonPackage(p) => &mut p.dependencies
        }
    }
    pub fn dependents(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3] {
        match self {
            PackageSymbol::Module(m) => &m.dependents,
            PackageSymbol::PythonPackage(p) => &p.dependents
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3] {
        match self {
            PackageSymbol::Module(m) => &mut m.dependents,
            PackageSymbol::PythonPackage(p) => &mut p.dependents
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
    pub name: String,
    pub path: String,
    pub i_ext: String,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub in_workspace: bool,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
    pub dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4],
    pub dependents: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3],

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<String, Vec<Rc<RefCell<Symbol>>>>,
}

impl PythonPackageSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        let mut res = Self {
            name,
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
            module_symbols: HashMap::new(),
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            dependencies: [
                vec![ //ARCH
                    PtrWeakHashSet::new() //ARCH
                ],
                vec![ //ARCH_EVAL
                    PtrWeakHashSet::new() //ARCH
                ],
                vec![
                    PtrWeakHashSet::new(), // ARCH
                    PtrWeakHashSet::new(), //ARCH_EVAL
                    PtrWeakHashSet::new()  //ODOO
                ],
                vec![
                    PtrWeakHashSet::new(), // ARCH
                    PtrWeakHashSet::new(), //ARCH_EVAL
                    PtrWeakHashSet::new()  //ODOO
                ]],
            dependents: [
                vec![ //ARCH
                    PtrWeakHashSet::new(), //ARCH
                    PtrWeakHashSet::new(), //ARCH_EVAL
                    PtrWeakHashSet::new(), //ODOO
                    PtrWeakHashSet::new(), //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    PtrWeakHashSet::new(), //ODOO
                    PtrWeakHashSet::new() //VALIDATION
                ],
                vec![ //ODOO
                    PtrWeakHashSet::new(), //ODOO
                    PtrWeakHashSet::new()  //VALIDATION
                ]],
        };
        res._init_symbol_mgr();
        res
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

}