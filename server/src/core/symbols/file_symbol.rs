use generational_arena::Index;
use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps}, core::model::Model, threads::SessionInfo};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::{Rc, Weak}};

use super::{symbol::Symbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub struct FileSymbol {
    pub name: String,
    pub path: String,
    pub is_external: bool,
    pub self_index: Option<Index>,
    pub parent: Option<Index>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub in_workspace: bool,
    pub model_dependencies: HashSet<Index>, //always on validation level, as odoo step is always required
    pub dependencies: [Vec<HashSet<Index>>; 4],
    pub dependents: [Vec<HashSet<Index>>; 3],

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Index>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<String, Vec<Rc<RefCell<Symbol>>>>,
}

impl FileSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        let mut res = Self {
            name,
            path,
            is_external,
            self_index: None,
            parent: None,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            model_dependencies: HashSet::new(),
            dependencies: [
                vec![ //ARCH
                    HashSet::new() //ARCH
                ],
                vec![ //ARCH_EVAL
                    HashSet::new() //ARCH
                ],
                vec![
                    HashSet::new(), // ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new()  //ODOO
                ],
                vec![
                    HashSet::new(), // ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new()  //ODOO
                ]],
            dependents: [
                vec![ //ARCH
                    HashSet::new(), //ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new(), //ODOO
                    HashSet::new(), //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    HashSet::new(), //ODOO
                    HashSet::new() //VALIDATION
                ],
                vec![ //ODOO
                    HashSet::new(), //ODOO
                    HashSet::new()  //VALIDATION
                ]],
        };
        res._init_symbol_mgr();
        res
    }

    pub fn add_symbol(&mut self, session: &mut SessionInfo, content: Index, section: u32) {
        let sections = self.symbols.entry(session.get_sym(content).unwrap().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

}