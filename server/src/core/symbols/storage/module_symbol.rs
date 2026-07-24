use super::symbol_mgr::SectionRange;
use crate::core::file_mgr::NoqaInfo;
use crate::core::model::Model;
use crate::core::symbols::SymbolTable;
use crate::core::symbols::storage::dependency_mgr::{DependenciesTable, DependentsTable};
use crate::core::symbols::symbol_keys::{JsFileKey, ModuleKey, NamespaceKey, SourceFileKey, SymbolKey, XmlId};
use super::symbol_mgr::SymbolMgr;
use crate::utils::{PathSanitizer};
use crate::weak_collections::WeakSet;
use crate::{constants::*, oyarn};
use ruff_text_size::TextRange;
use std::cell::RefCell;
use crate::utils::{HashMap, HashSet};
use std::path::Path;
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
    pub(in crate::core::symbols) data: Vec<(String, TextRange)>,
    pub(in crate::core::symbols) assets: Vec<(String, TextRange)>,
    pub xml_ids: HashMap<OYarn, WeakSet<XmlId>>, //Reference to xmlNode declaring xml_id. Can be from python or xml file.
    pub (super) current_build_step: BuildSteps,
    pub (super) build_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_data: HashMap<String, BuildSteps>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub not_found_data_ids: HashMap<MissingDataSource, BuildSteps>,
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
    pub(super) fs_symbols: HashMap<OYarn, SymbolKey>,
    pub(super) data_file_symbols: HashMap<String, SourceFileKey>,
    pub(super) js_symbols: HashMap<String, JsFileKey>,
}

impl ModuleSymbol {

    pub fn new(name: &str, dir_path: &Path, parent: NamespaceKey, is_external: bool) -> Self {
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
            not_found_data_ids: HashMap::default(),
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
            fs_symbols: HashMap::default(),
            current_build_step: BuildSteps::ARCH,
            build_status: BuildStatus::PENDING,
            sections: vec![],
            symbols: HashMap::default(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
            data_file_symbols: HashMap::default(),
            js_symbols: HashMap::default(),
        };
        module._init_symbol_mgr();
        module
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.fs_symbols
    }

    pub fn data(&self) -> &[(String, TextRange)] {
        &self.data
    }

    pub fn data_file_symbols(&self) -> &HashMap<String, SourceFileKey> {
        &self.data_file_symbols
    }

    pub fn js_symbols(&self) -> &HashMap<String, JsFileKey> {
        &self.js_symbols
    }

    pub fn parent(&self) -> NamespaceKey {
        self.parent
    }

    pub fn is_in_deps(symbol_table: &SymbolTable, module_key: ModuleKey, dir_name: &OYarn) -> bool {
        let module = &symbol_table[module_key];
        module.dir_name == *dir_name || module.all_depends.contains(dir_name)
    }

    pub fn get_all_depends(&self) -> &HashSet<OYarn> {
        &self.all_depends
    }

    pub fn insert_xml_id(symbol_table: &mut SymbolTable, target: ModuleKey, xml_id: OYarn, xml_data: XmlId) {
        symbol_table[target].xml_ids.entry(xml_id).or_default().insert(xml_data);
    }

    //given an xml_id without "module." part, return all XmlData that declare it ("this_module.xml_id"), regardless of the module declaring it.
    pub fn get_xml_id(symbol_table: &SymbolTable, target: ModuleKey, xml_id: &str) -> Option<WeakSet<XmlId>> {
        let target_module = &symbol_table[target];
        target_module.xml_ids.get(xml_id).cloned()
    }

}
