use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, MissingDataSource, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::{Buildable, storage::dependency_mgr::{DependenciesTable, DependentsTable}, symbol_keys::{JsFileParent, SymbolKey}}}, oyarn, utils::HashMap};
use std::{cell::RefCell, rc::Weak};

#[derive(Debug)]
pub struct JsFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    parent: JsFileParent,
    pub arch_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_data_ids: HashMap<MissingDataSource, BuildSteps>,
    pub (super) in_workspace: bool,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,
}

impl JsFileSymbol {

    pub fn new(name: &str, path: &str, parent: JsFileParent, is_external: bool) -> Self {
        
        Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            parent,
            arch_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_data_ids: HashMap::default(),
            in_workspace: false,
            self_import: false,
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        }
    }

    pub fn parent(&self) -> JsFileParent {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }

}

impl Buildable for JsFileSymbol {
    fn build_status(&self, step: BuildSteps) -> BuildStatus {
        match step {
            BuildSteps::SYNTAX => panic!(),
            BuildSteps::ARCH => self.arch_status,
            BuildSteps::ARCH_EVAL => panic!(),
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
