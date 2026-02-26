use weak_table::{PtrWeakHashSet, PtrWeakKeyHashMap};

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::symbol_table::SymbolKey, xml_data::OdooData}, oyarn};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::{Rc, Weak}};

use super::{symbol::Symbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub struct FileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    // pub weak_self: Option<Weak<RefCell<Symbol>>>,
    // @arena: probably always Some. Maybe remove the option.
    pub parent: Option<SymbolKey>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>, //used for dynamic XML_ID records, like ir.models
    pub (super) in_workspace: bool,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<HashSet<SymbolKey>>>>,
    pub dependents: Vec<Vec<Option<HashSet<SymbolKey>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>,
}

impl FileSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            // weak_self: None,
            parent: Some(parent),
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            xml_ids: HashMap::new(),
            in_workspace: false,
            self_import: false,
            sections: vec![],
            symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            not_found_models: HashMap::new(),
            noqas: NoqaInfo::None,
        };
        res._init_symbol_mgr();
        res
    }

    // @arena: moved to SymbolMgr
    // pub fn add_symbol(&mut self, content: SymbolKey, name: &OYarn, section: u32) {
    //     let sections = self.symbols.entry(name.clone()).or_insert_with(|| HashMap::new());
    //     let section_vec = sections.entry(section).or_insert_with(|| vec![]);
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