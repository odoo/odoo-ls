use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::NoqaInfo, model::Model, symbols::{storage::{dependency_mgr::{DependenciesTable, DependentsTable}, FileSystemSymbolParent}, symbol_keys::SymbolKey}}, oyarn, utils::PathSanitizer};
use std::{cell::RefCell, path::Path, rc::Weak};
use crate::utils::HashMap;

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
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,

    // parent / child symbols
    parent: FileSystemSymbolParent,
    pub(super) symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>,
    pub(super) fs_symbols: HashMap<OYarn, SymbolKey>,
}

impl PythonPackageSymbol {

    pub fn new(name: &str, path: &str, parent: FileSystemSymbolParent, is_external: bool, i_ext: &'static str) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            init_path: Path::new(path).join("__init__.py").sanitize() + i_ext,
            is_external,
            i_ext,
            parent,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            self_import: false, //indicates that if unloaded, the symbol should be added in the rebuild automatically as nothing depends on it (used for root packages)
            fs_symbols: HashMap::default(),
            sections: vec![],
            symbols: HashMap::default(),
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
        &self.fs_symbols
    }

    pub fn parent(&self) -> FileSystemSymbolParent {
        self.parent
    }

}
