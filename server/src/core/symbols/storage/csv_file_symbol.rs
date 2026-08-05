use weak_table::PtrWeakHashSet;

use crate::constants::MissingDataSource;
use crate::core::symbols::storage::dependency_mgr::{DependenciesTable, DependentsTable};
use crate::core::symbols::symbol_keys::{ModuleKey, XmlDataKey};
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::symbol_keys::SymbolKey}, oyarn};
use crate::utils::{HashMap, HashSet};
use std::{cell::RefCell, rc::Weak};

#[derive(Debug)]
pub struct CsvFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    pub (super) current_build_step: BuildSteps,
    pub (super) build_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_data_ids: HashMap<MissingDataSource, BuildSteps>,
    pub (super) in_workspace: bool,
    pub (super) symbols: HashSet<XmlDataKey>,
    pub model_name: OYarn,
    pub headers: Vec<OYarn>,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    // parent symbol (no child symbols)
    parent: ModuleKey,
}

impl CsvFileSymbol {

    pub fn new(name: &str, path: &str, parent: ModuleKey, is_external: bool) -> Self {
        
        Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            parent,
            current_build_step: BuildSteps::ARCH,
            build_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_data_ids: HashMap::default(),
            in_workspace: false,
            model_name: OYarn::default(),
            headers: Vec::new(),
            symbols: HashSet::default(),
            self_import: false,
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        }
    }

    pub fn parent(&self) -> ModuleKey {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        self.symbols.iter().copied().map(SymbolKey::from).collect()
    }

    pub fn symbols(&self) -> &HashSet<XmlDataKey> {
        &self.symbols
    }

}
