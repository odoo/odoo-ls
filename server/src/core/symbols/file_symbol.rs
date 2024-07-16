use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, SymType}, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, collections::HashMap, rc::{Rc, Weak}};

use super::{symbol::Symbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub struct FileSymbol {
    pub name: String,
    pub path: String,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub in_workspace: bool,
    pub dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4],
    pub dependents: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3],

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
}

impl FileSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        let mut res = Self {
            name,
            path,
            is_external,
            weak_self: None,
            parent: None,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
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