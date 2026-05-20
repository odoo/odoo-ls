use super::symbol_mgr::SectionRange;
use crate::core::file_mgr::NoqaInfo;
use crate::core::model::Model;
use crate::core::symbols::storage::dependency_mgr::{DependenciesTable, DependentsTable};
use crate::core::symbols::symbol_keys::{NamespaceKey, SourceFileKey, SymbolKey, XmlId};
use super::symbol_mgr::SymbolMgr;
use crate::threads::SessionInfo;
use crate::utils::{PathSanitizer};
use crate::weak_collections::WeakSet;
use crate::{constants::*, oyarn};
use ruff_text_size::TextRange;
use std::cell::RefCell;
use crate::utils::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Weak;
use weak_table::PtrWeakHashSet;

#[derive(Debug)]
pub struct ModuleSymbol {
    pub name: OYarn,
    pub path: String,
    pub init_path: String,
    pub is_external: bool,
    pub(in crate::core::symbols) root_path: String,
    pub(in crate::core::symbols) loaded: bool,
    pub module_name: OYarn,
    pub dir_name: OYarn,
    pub depends: Vec<(OYarn, TextRange)>,
    pub(in crate::core::symbols) all_depends: HashSet<OYarn>, //computed all depends to avoid too many recomputations
    pub(in crate::core::symbols) data: Vec<(String, TextRange)>, // TODO
    pub(in crate::core::symbols) assets: Vec<(String, TextRange)>,
    pub xml_ids: HashMap<OYarn, WeakSet<XmlId>>, //Reference to xmlNode declaring xml_id. Can be from python or xml file.
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_data: HashMap<String, BuildSteps>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub(super) in_workspace: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,

    // parent / child symbols
    parent: NamespaceKey,
    pub(super) symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
    pub(super) data_symbols: HashMap<String, SourceFileKey>,
}

impl ModuleSymbol {

    pub fn new(session: &mut SessionInfo, name: &str, dir_path: &PathBuf, parent: NamespaceKey, is_external: bool) -> Option<Self> {
        let path = dir_path.sanitize();
        let dir_name = oyarn!("{}", dir_path.with_extension("").components().next_back().unwrap().as_os_str().to_str().unwrap());
        let depends = if dir_name == "base" {
            vec![]
        } else {
            vec![(OYarn::from("base"), TextRange::default())]
        };
        let mut module = ModuleSymbol {
            name: oyarn!("{}", name),
            path: path.clone(),
            init_path: dir_path.join("__init__.py").sanitize(),
            is_external,
            not_found_paths: vec![],
            not_found_data: HashMap::default(),
            not_found_models: HashMap::default(),
            in_workspace: false,
            root_path: path,
            loaded: false,
            module_name: OYarn::from(""),
            xml_ids: HashMap::default(),
            dir_name,
            depends,
            all_depends: HashSet::default(),
            data: Vec::new(),
            assets: Vec::new(),
            parent,
            module_symbols: HashMap::default(),
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            sections: vec![],
            symbols: HashMap::default(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
            data_symbols: HashMap::default(),
        };
        module._init_symbol_mgr();
        module.load(session, dir_path)
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.module_symbols
    }

    pub fn data_symbols(&self) -> &HashMap<String, SourceFileKey> {
        &self.data_symbols
    }

    pub fn parent(&self) -> NamespaceKey {
        self.parent
    }

    /// symbols + module_symbols + data_symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        self.symbols.values().flat_map(|section| section.values()).flatten()
            .chain(self.module_symbols.values()).copied()
            .chain(self.data_symbols.values().map(|&key| key.into()))
            .collect()
    }

}
