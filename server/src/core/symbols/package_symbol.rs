use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, SymType}, threads::SessionInfo, S};
use std::{cell::{RefCell, RefMut}, collections::HashMap, rc::{Rc, Weak}};

use super::{module_symbol::ModuleSymbol, symbol::MainSymbol, symbol_mgr::SectionRange};

#[derive(Debug)]
pub enum PackageSymbol {
    PythonPackage(PythonPackageSymbol),
    Module(ModuleSymbol)
}

impl PackageSymbol {
    pub fn new_python_package(name: String, path: String, is_external: bool) -> Self {
        PackageSymbol::PythonPackage(PythonPackageSymbol::new(name, path, is_external))
    }
    pub fn new_module_package(name: String, path: String, is_external: bool) -> Self {
        PackageSymbol::Module(ModuleSymbol::new(name, path, is_external))
    }
    pub fn name(&self) -> String {
        match self {
            PackageSymbol::PythonPackage(p) => p.name,
            PackageSymbol::Module(m) => m.name,
        }
    }
    pub fn parent(&self) -> Option<Weak<RefCell<MainSymbol>>> {
        match self {
            PackageSymbol::Module(m) => m.parent,
            PackageSymbol::PythonPackage(p) => p.parent
        }
    }
    pub fn set_parent(&mut self, parent: Weak<RefCell<MainSymbol>>) {
        match self {
            PackageSymbol::Module(m) => m.parent = Some(parent),
            PackageSymbol::PythonPackage(p) => p.parent = Some(parent),
        }
    }
    pub fn set_init_ext(&mut self, ext: String) {
        match self {
            PackageSymbol::PythonPackage(p) => {p.i_ext = ext},
            PackageSymbol::Module(m) => {m.i_ext = ext},
        }
    }
    pub fn dependencies(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 4] {
        match self {
            PackageSymbol::Module(m) => &m.dependencies,
            PackageSymbol::PythonPackage(p) => &p.dependencies
        }
    }
    pub fn dependencies_as_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 4] {
        match self {
            PackageSymbol::Module(m) => &mut m.dependencies,
            PackageSymbol::PythonPackage(p) => &mut p.dependencies
        }
    }
    pub fn dependents(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 3] {
        match self {
            PackageSymbol::Module(m) => &m.dependents,
            PackageSymbol::PythonPackage(p) => &p.dependents
        }
    }
    pub fn dependents_as_mut(&self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 3] {
        match self {
            PackageSymbol::Module(m) => &mut m.dependents,
            PackageSymbol::PythonPackage(p) => &mut p.dependents
        }
    }
    pub fn add_file(&mut self, file: Rc<RefCell<MainSymbol>>) {
        match self {
            PackageSymbol::Module(m) => m.module_symbols.insert(file.borrow().name().clone(), file),
            PackageSymbol::PythonPackage(p) => p.module_symbols.insert(file.borrow().name().clone(), file),
        };
    }
    pub fn paths(&self) -> Vec<String> {
        match self {
            PackageSymbol::Module(m) => vec![m.path],
            PackageSymbol::PythonPackage(p) => vec![p.path],
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
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub in_workspace: bool,
    pub module_symbols: HashMap<String, Rc<RefCell<MainSymbol>>>,
    pub dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 4],
    pub dependents: [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 3],

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<MainSymbol>>>>>,
}

impl PythonPackageSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
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
        }
    }

}