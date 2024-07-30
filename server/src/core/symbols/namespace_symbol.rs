use weak_table::PtrWeakHashSet;

use crate::{constants::SymType, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, collections::HashMap, rc::{Rc, Weak}};

use super::symbol::MainSymbol;


#[derive(Debug)]
struct NamespaceDirectory {
    pub path: String,
    pub module_symbols: HashMap<String, Vec<Rc<RefCell<MainSymbol>>>>,
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: String,
    pub directories: Vec<NamespaceDirectory>,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub in_workspace: bool,
    pub module_symbols: HashMap<String, Rc<RefCell<MainSymbol>>>,
    pub dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 4],
    pub dependents: [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 3],
}

impl NamespaceSymbol {

    pub fn new(name: String, paths: Vec<String>, is_external: bool) -> Self {
        Self {
            name,
            directories: vec![],
            is_external,
            weak_self: None,
            parent: None,
            in_workspace: false,
            module_symbols: HashMap::new(),
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

    pub fn add_file(&mut self, file: Rc<RefCell<MainSymbol>>) {
        self.module_symbols.insert(file.borrow().name().clone(), file);
    }

    pub fn paths(&self) -> Vec<String> {
        self.directories.iter().map(|x| {x.path}).collect()
    }

}