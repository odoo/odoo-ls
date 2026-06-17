use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, DataType, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::{storage::dependency_mgr::{DependenciesTable, DependentsTable}, symbol_keys::SymbolKey}}, oyarn};
use std::{cell::RefCell, rc::Weak};
use crate::utils::HashMap;

use super::symbol_mgr::{SectionRange, SymbolMgr};

#[derive(Debug)]
pub struct FileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub not_found_data_ids: HashMap<DataType, BuildSteps>,
    pub (super) in_workspace: bool,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,

    // parent / child symbols
    parent: SymbolKey,
    pub(super) symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>,
}

impl FileSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            parent,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_data_ids: HashMap::default(),
            in_workspace: false,
            self_import: false,
            sections: vec![],
            symbols: HashMap::default(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            not_found_models: HashMap::default(),
            noqas: NoqaInfo::None,
        };
        res._init_symbol_mgr();
        res
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        self.symbols.values()
            .flat_map(|section| section.values())
            .flatten()
            .copied().collect()
    }

}
