use weak_table::PtrWeakHashSet;

use crate::core::symbols::Buildable;
use crate::core::symbols::storage::dependency_mgr::{DependenciesTable, DependentsTable};
use crate::core::symbols::symbol_keys::ModuleKey;
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::symbol_keys::SymbolKey, xml_data::OdooData}, oyarn};
use std::{cell::RefCell, collections::HashMap, rc::Weak};

#[derive(Debug)]
pub struct CsvFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    pub arch_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub (super) in_workspace: bool,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>,
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
        let res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            parent,
            arch_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            model_name: OYarn::default(),
            headers: Vec::new(),
            xml_ids: HashMap::new(),
            self_import: false,
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

    /// no child symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
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
