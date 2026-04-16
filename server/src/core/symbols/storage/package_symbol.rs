use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::{storage::dependency_mgr::{DependenciesTable, DependentsTable}, symbol_keys::SymbolKey}, xml_data::OdooData}, oyarn, utils::{NoHashBuilder, PathSanitizer}};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Weak};

use super::symbol_mgr::{SectionRange, SymbolMgr};

#[derive(Debug)]
pub struct PythonPackageSymbol {
    pub name: OYarn,
    pub path: String,
    pub init_path: String,
    pub i_ext: &'static str,
    pub is_external: bool,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub(super) in_workspace: bool,
    pub self_import: bool,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>, //used for dynamic XML_ID records, like ir.models
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,

    // parent / child symbols
    parent: SymbolKey,
    pub(super) symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>, NoHashBuilder>>,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

impl PythonPackageSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool, i_ext: &'static str) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            init_path: PathBuf::from(path).join("__init__.py").sanitize() + i_ext,
            is_external,
            i_ext,
            parent,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            xml_ids: HashMap::new(),
            self_import: false, //indicates that if unloaded, the symbol should be added in the rebuild automatically as nothing depends on it (used for root packages)
            module_symbols: HashMap::new(),
            sections: vec![],
            symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res._init_symbol_mgr();
        res
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.module_symbols
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    /// symbols + module_symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        self.symbols.values().flat_map(|section| section.values()).flatten()
            .chain(self.module_symbols.values())
            .copied().collect()
    }

}
