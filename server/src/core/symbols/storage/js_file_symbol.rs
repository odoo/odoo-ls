use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, DataType, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::{storage::dependency_mgr::{DependenciesTable, DependentsTable}, symbol_keys::{ModuleKey, SymbolKey}}}, oyarn, utils::HashMap};
use std::{cell::RefCell, rc::Weak};

#[derive(Debug)]
pub struct JsFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    parent: ModuleKey,
    pub arch_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_data_ids: HashMap<DataType, BuildSteps>,
    pub (super) in_workspace: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,
}

impl JsFileSymbol {

    pub fn new(name: &str, path: &str, parent: ModuleKey, is_external: bool) -> Self {
        let res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            parent,
            arch_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_data_ids: HashMap::default(),
            in_workspace: false,
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res
    }

    pub fn parent(&self) -> ModuleKey {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }

}