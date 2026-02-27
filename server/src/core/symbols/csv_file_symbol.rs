use weak_table::PtrWeakHashSet;

use crate::core::symbols::dependency_mgr::Buildable;
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::symbol_table::SymbolKey, xml_data::OdooData}, oyarn};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::{Rc, Weak}};

use super::{symbol::Symbol, symbol_mgr::SectionRange};

#[derive(Debug)]
pub struct CsvFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    // pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<SymbolKey>,
    pub arch_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub (super) in_workspace: bool,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>,
    pub model_name: OYarn,
    pub headers: Vec<OYarn>,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<HashSet<SymbolKey>>>>,
    pub dependents: Vec<Vec<Option<HashSet<SymbolKey>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    // @arena: dead code
    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>,
}

impl CsvFileSymbol {

    // @arena: parent is always package(module)
    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        let res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            // weak_self: None,
            parent: Some(parent),
            arch_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            model_name: OYarn::default(),
            headers: Vec::new(),
            xml_ids: HashMap::new(),
            self_import: false,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res
    }

    // @arena: dead code
    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

}

impl Buildable for CsvFileSymbol {
    fn build_status(&self, step: BuildSteps) -> BuildStatus {
        match step {
            BuildSteps::SYNTAX => panic!(),
            BuildSteps::ARCH => self.arch_status,
            BuildSteps::ARCH_EVAL => self.arch_status,
            BuildSteps::VALIDATION => self.validation_status,
        }
    }
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        match step {
            BuildSteps::SYNTAX => panic!(),
            BuildSteps::ARCH => self.arch_status = status,
            BuildSteps::ARCH_EVAL => panic!(),
            BuildSteps::VALIDATION => self.validation_status = status,
        }
    }
}