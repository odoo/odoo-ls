use crate::constants::OYarn;
use crate::core::build_scheduler::BuildScheduler;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::entry_point::EntryPointType;
use crate::core::file_mgr::{Ast, PreloadedFile};
use crate::core::js_arch_builder::ComponentDescriptor;
use crate::core::js_type_files;
use crate::core::module_load_order::sort_by_load_order;
use crate::core::pre_parser::{PreParseCache, PreParser};
use crate::core::symbols::ModuleSymbol;
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::storage::metrics::{log_slotmap_capacities, log_symbol_counts, log_memory_usage};
use crate::core::symbols::symbol_keys::{BuildableSymbolKey, FunctionKey, ModuleKey, SourceFileKey, SymbolKey, Wk, XmlId, XmlTemplateKey};
use crate::core::tsserver_bridge::{TsServerBridge};
use crate::features::tsserver_completion::TsCompletionResolveData;
use crate::features::owl_virtual;
use crate::fifo_ptr_weak_hash_set::FifoWeakHashSet;
use crate::lsp_types_custom::{ConfigDiagnosticAction, ConfigDiagnosticMessage, ConfigDiagnosticMessageLevel};
use crate::odoo_version::OdooVersion;
use crate::features::document_symbols::DocumentSymbolFeature;
use crate::features::references::{ReferenceFeature, ReferenceTarget};
use crate::features::workspace_symbols::WorkspaceSymbolFeature;
use crate::features::declaration::DeclarationFeature;
use crate::features::completion::CompletionFeature;
use crate::features::definition::DefinitionFeature;
use crate::features::hover::HoverFeature;
use crate::progress_reporter::ProgressReporterPercentage;
use crate::threads::{SessionInfo, ThreadMessage, TsServerDiagnostics};
use crate::features::semantic_tokens::SemanticTokensFeature;
use crate::weak_collections::{WeakMap, WeakSet};
use crate::utils::{HashMap, is_dir_cs, is_file_cs};
use std::cell::RefCell;
use std::rc::{Rc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use lsp_server::{ErrorCode, RequestId, ResponseError};
use lsp_types::request::GotoDeclarationResponse;
use lsp_types::*;
use request::{RegisterCapability, Request, WorkspaceConfiguration};
use ruff_source_file::PositionEncoding;
use serde_json::Value;
use tracing::{error, warn, info};

use crate::utils::HashSet;
use std::process::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use regex::Regex;
use crate::{constants::*, Sy};
use crate::tree::{Tree, TreeStrSlice};
use super::config::{self, DEFAULT_PROFILE_NAME, get_configuration, ConfigEntry, ConfigView};
use super::entry_point::{EntryPoint, EntryPointMgr};
use super::file_mgr::FileMgr;
use super::import_resolver::ImportCache;
use crate::core::model::Model;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::utils::{PathSanitizer, ToFilePath as _, expand_language_code};
use crate::S;
//use super::python_arch_builder::PythonArchBuilder;

static VERSION_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"version_info = \((['\"]?(\D+~)?\d+['\"]?, \d+, \d+, \w+, \d+, \D+)\)"#).unwrap()
});

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq)]
pub enum InitState {
    NOT_READY,
    PYTHON_READY,
    ODOO_READY,
}

#[derive(Debug)]
pub struct TypeshedWeakReferences {
    dict: Wk<SymbolKey>,
    tuple: Wk<SymbolKey>,
    set: Wk<SymbolKey>,
    list: Wk<SymbolKey>,
    string: Wk<SymbolKey>,
    boolean: Wk<SymbolKey>,
    int: Wk<SymbolKey>,
    float: Wk<SymbolKey>,
    complex: Wk<SymbolKey>,
    ellipsis: Wk<SymbolKey>,
    bytes: Wk<SymbolKey>,
    object: Wk<SymbolKey>,
}

impl Default for TypeshedWeakReferences {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeshedWeakReferences {

    pub fn new() -> Self {
        Self {
            dict: Wk::null(),
            tuple: Wk::null(),
            set: Wk::null(),
            list: Wk::null(),
            string: Wk::null(),
            boolean: Wk::null(),
            int: Wk::null(),
            float: Wk::null(),
            complex: Wk::null(),
            ellipsis: Wk::null(),
            bytes: Wk::null(),
            object: Wk::null(),
        }
    }
}

#[derive(Debug)]
pub struct SyncOdoo {
    pub version: OdooVersion,
    pub python_version: Vec<u32>,
    /// Filename suffixes the configured Python interpreter accepts for C
    /// extension modules, in CPython's own probe order (the value of
    /// `importlib.machinery.EXTENSION_SUFFIXES`). Used to detect compiled
    /// modules without globbing. Populated from the Python query in `init`;
    /// initialized to a platform default so non-Python init paths still work.
    pub python_ext_suffixes: Vec<String>,
    pub config: ConfigEntry,
    pub config_file: Option<ConfigView>,
    pub config_path: Option<String>,
    pub selected_config: Option<String>,
    pub entry_point_mgr: Rc<RefCell<EntryPointMgr>>, //An Rc to be able to clone it and free session easily
    pub has_main_entry:bool,
    pub has_odoo_main_entry: bool,
    pub has_valid_python: bool,
    pub main_entry_tree: Vec<OYarn>,
    pub stubs_dirs: Vec<String>,
    pub stdlib_dir: String,
    pub progress_token: i32,
    file_mgr: Rc<RefCell<FileMgr>>,
    pub modules: HashMap<OYarn, Wk<ModuleKey>>,
    pub models: HashMap<OYarn, Rc<RefCell<Model>>>,
    pub interrupt_rebuild: Arc<AtomicBool>,
    pub terminate_rebuild: Arc<AtomicBool>,
    pub current_request_id: Option<RequestId>,
    pub running_request_ids: Arc<Mutex<Vec<RequestId>>>, //Arc to Server mutex for cancellation support
    pub watched_file_updates: u32,
    pub build_scheduler: BuildScheduler,
    pub state_init: InitState,
    pub must_reload_paths: Vec<(Wk<SymbolKey>, String)>, // formerly Weak refs
    pub load_odoo_addons: bool, //indicate if we want to load odoo addons or not
    pub need_rebuild: bool, //if true, the next process_rebuilds will drop everything and rebuild everything
    pub import_cache: Option<ImportCache>,
    pub capabilities: lsp_types::ClientCapabilities,
    pub encoding: PositionEncoding,
    pub opened_files: Vec<String>,
    pub symbol_table: SymbolTable,
    pub evaluation_search: Option<ReferenceTarget>, //If set, any evaluation will be check against this value. If evaluation matches, location is kept in evaluation_locations
    pub evaluation_locations: Vec<Location>,
    pub typeshed_weak_cache: TypeshedWeakReferences, //cache of weak references to important typeshed symbols, to avoid having to look for them in the graph for each evaluation
    pub deferred_subfunc_invalidation: Option<FifoWeakHashSet<SourceFileKey>>, // None = eager (default)
    languages_by_source: WeakMap<SourceFileKey, HashSet<String>>,
    language_dependents: WeakSet<SymbolKey>,
    pre_parse_cache: Option<Arc<PreParseCache>>, // used by `build_modules`

    pub test_mode: bool,

    pub tsserver_bridge: Option<TsServerBridge>,
    pub js_templates: HashMap<String, WeakSet<XmlTemplateKey>>,
    pub component_descriptors: HashMap<String, ComponentDescriptor>,
    /// Template name → every class declaring it. Resolve with
    /// [`js_component_index::component_for_template`].
    pub js_component_by_template: HashMap<String, Vec<String>>,
}

unsafe impl Send for SyncOdoo {}

impl Default for SyncOdoo {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncOdoo {

    pub fn new() -> Self {

        Self {
            version: OdooVersion::default(),
            python_version: vec![0, 0, 0],
            python_ext_suffixes: Vec::new(),
            config: ConfigEntry::new(),
            selected_config: None,
            config_file: None,
            config_path: None,
            entry_point_mgr: Rc::new(RefCell::new(EntryPointMgr::new())),
            has_main_entry: false,
            has_odoo_main_entry: false,
            has_valid_python: false,
            main_entry_tree: vec![],
            progress_token: 0,
            file_mgr: Rc::new(RefCell::new(FileMgr::new())),
            stubs_dirs: SyncOdoo::default_stubs(),
            stdlib_dir: SyncOdoo::default_stdlib(),
            modules: HashMap::default(),
            models: HashMap::default(),
            interrupt_rebuild: Arc::new(AtomicBool::new(false)),
            terminate_rebuild: Arc::new(AtomicBool::new(false)),
            current_request_id: None,
            running_request_ids: Arc::new(Mutex::new(vec![])),
            watched_file_updates: 0,
            build_scheduler: BuildScheduler::new(),
            state_init: InitState::NOT_READY,
            must_reload_paths: vec![],
            load_odoo_addons: true,
            need_rebuild: false,
            import_cache: None,
            capabilities: lsp_types::ClientCapabilities::default(),
            encoding: PositionEncoding::Utf16,
            opened_files: vec![],
            symbol_table: SymbolTable::new(),
            evaluation_search: None,
            evaluation_locations: vec![],
            typeshed_weak_cache: TypeshedWeakReferences::new(),
            languages_by_source: WeakMap::new(),
            language_dependents: WeakSet::new(),
            deferred_subfunc_invalidation: None,
            pre_parse_cache: None,

            test_mode: false,
            tsserver_bridge: None,
            js_templates: HashMap::default(),
            component_descriptors: HashMap::default(),
            js_component_by_template: HashMap::default(),
        }
    }

    pub fn reset(session: &mut SessionInfo, config: ConfigEntry) {
        session.log_message(MessageType::INFO, S!("Resetting Database..."));
        info!("Resetting database...");
        session.sync_odoo.version = OdooVersion::default();
        session.sync_odoo.config = ConfigEntry::new();
        FileMgr::clear(session);//only reset files, as workspace folders didn't change
        session.sync_odoo.stubs_dirs = SyncOdoo::default_stubs();
        session.sync_odoo.stdlib_dir = SyncOdoo::default_stdlib();
        session.sync_odoo.modules = HashMap::default();
        session.sync_odoo.models = HashMap::default();
        session.sync_odoo.build_scheduler = BuildScheduler::new();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.sync_odoo.load_odoo_addons = true;
        session.sync_odoo.need_rebuild = false;
        session.sync_odoo.watched_file_updates = 0;
        session.sync_odoo.languages_by_source = WeakMap::new();
        session.sync_odoo.language_dependents = WeakSet::new();
        session.sync_odoo.tsserver_bridge = None;
        //drop all entries, except entries of opened files
        session.sync_odoo.entry_point_mgr.borrow_mut().reset_entry_points(&mut session.sync_odoo.symbol_table, false);
        SyncOdoo::init(session, config);
    }

    pub fn default_stdlib() -> String {
        let next_to_exe = env::current_exe().unwrap().parent().unwrap().join("typeshed").join("stdlib");
        if next_to_exe.exists() {
            next_to_exe.sanitize()
        } else {
            env::current_dir().unwrap().join("typeshed").join("stdlib").sanitize()
        }
    }

    pub fn default_stubs() -> Vec<String> {
        let mut result = vec![];
        let next_to_exe = env::current_exe().unwrap().parent().unwrap().join("typeshed").join("stubs");
        if next_to_exe.exists() {
            result.push(next_to_exe.sanitize());
        } else {
            result.push(env::current_dir().unwrap().join("typeshed").join("stubs").sanitize());
        }
        let next_to_exe = env::current_exe().unwrap().parent().unwrap().join("additional_stubs");
        if next_to_exe.exists() {
            result.push(next_to_exe.sanitize());
        } else {
            result.push(env::current_dir().unwrap().join("additional_stubs").sanitize());
        }
        result
    }

    pub fn init(session: &mut SessionInfo, config: ConfigEntry) {
        #[cfg(debug_assertions)]
        crate::constants::install_debug_ptr(&session.sync_odoo.symbol_table);

        session.sync_odoo.symbol_table.pre_allocate();
        info!("Initializing odoo");
        info!("Full Config: {:?}", config);
        let start_time = Instant::now();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.send_notification("$Odoo/loadingStatusUpdate", "start");
        session.sync_odoo.config = config;
        if session.sync_odoo.config.no_typeshed_stubs() {
            session.sync_odoo.stubs_dirs.clear();
        }
        for stub in session.sync_odoo.config.additional_stubs().iter() {
            session.sync_odoo.stubs_dirs.push(Path::new(stub).sanitize());
        }
        if !session.sync_odoo.config.stdlib().is_empty() {
            session.sync_odoo.stdlib_dir = Path::new(&session.sync_odoo.config.stdlib()).sanitize();
        }
        info!("Using stdlib path: {}", session.sync_odoo.stdlib_dir);
        for stub in session.sync_odoo.stubs_dirs.iter() {
            let path = Path::new(stub);
            let found = match path.exists() {
                true  => "found",
                false => "not found",
            };
            info!("stub {:?} - {}", stub, found)
        }
        'python_check: {
            EntryPointMgr::add_entry_to_builtins(session, session.sync_odoo.stdlib_dir.clone());
            for stub_dir in session.sync_odoo.stubs_dirs.clone().iter() {
                EntryPointMgr::add_entry_to_public(session, stub_dir.clone());
            }
            match Command::new(session.sync_odoo.config.python_path().clone()).args(["-c", "import sys; import json; print(json.dumps(sys.path))"]).output() {
                Err(err) => {
                    warn!("Wrong python command: {}, error: {}", session.sync_odoo.config.python_path().clone(), err);
                    session.send_notification("$Odoo/invalid_python_path", ());
                    break 'python_check;
                },
                Ok(output) => {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        session.log_message(MessageType::INFO, format!("Detected sys.path: {}", stdout));
                        let paths: Vec<String> = serde_json::from_str(&stdout).expect("Unable to get paths with json of sys.path output");
                        for path in paths.iter() {
                            let path = path.replace("\\\\", "\\");
                            let pathbuf = Path::new(&path);
                            if pathbuf.is_dir() {
                                let final_path = pathbuf.sanitize_cow();
                                session.log_message(MessageType::INFO, format!("Adding sys.path: {}", final_path));
                                EntryPointMgr::add_entry_to_public(session, final_path.to_string());
                            }
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        warn!("Error reading sys.path: {}", stderr);
                    }
                }
            }
            match Command::new(session.sync_odoo.config.python_path().clone()).args(["-c", "import sys, importlib.machinery, json; print(json.dumps({'version_info': list(sys.version_info)[:3], 'ext_suffixes': list(importlib.machinery.EXTENSION_SUFFIXES)}))"]).output() {
                Err(err) => {
                    warn!("Wrong python command: {}, error: {}", session.sync_odoo.config.python_path().clone(), err);
                    session.send_notification("$Odoo/invalid_python_path", ());
                },
                Ok(output) => {
                    session.sync_odoo.has_valid_python = true;
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        session.log_message(MessageType::INFO, format!("Detected python info: {}", stdout));
                        let info: Value = serde_json::from_str(&stdout).expect("Unable to parse python info json");
                        session.sync_odoo.python_version = info["version_info"].as_array()
                            .expect("Expected JSON array for version_info")
                            .iter()
                            .filter_map(|v| v.as_u64())
                            .map(|v| v as u32)
                            .take(3)
                            .collect();
                        session.sync_odoo.python_ext_suffixes = info["ext_suffixes"].as_array()
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default();
                        info!("Detected python version: {}.{}.{}", session.sync_odoo.python_version[0], session.sync_odoo.python_version[1], session.sync_odoo.python_version[2]);
                        info!("Detected python EXTENSION_SUFFIXES: {:?}", session.sync_odoo.python_ext_suffixes);
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        warn!("Error reading python info: {}", stderr);
                    }
                }
            }
        }
        let tsserver_handle = if session.sync_odoo.config.is_javascript_disabled() {
            None
        } else {
            session.clone_sender_to_main().map(|sender| {
                let tsserver_cmd = session.sync_odoo.config.tsserver_command();
                let odoo_path = session.sync_odoo.config.odoo_path().clone();
                let addons_paths = session.sync_odoo.config.addons_paths().iter().cloned().collect::<Vec<_>>();
                let ts_check = session.sync_odoo.config.ts_check();
                std::thread::spawn(move || {
                    SyncOdoo::setup_and_start_tsserver(tsserver_cmd, sender, odoo_path, addons_paths, ts_check)
                })
            })
        };
        if SyncOdoo::load_builtins(session) {
            session.sync_odoo.state_init = InitState::PYTHON_READY;
            SyncOdoo::build_database(session);
        }
        if let Some(handle) = tsserver_handle {
            match handle.join() {
                Ok(Some(bridge)) => session.sync_odoo.tsserver_bridge = Some(bridge),
                Ok(None) => {
                    let tsserver_command = session.sync_odoo.config.tsserver_command();
                    if !tsserver_command.is_empty() { //if command is empty, do not show diagnostic, as the user probably wanted to disable tsserver
                        if tsserver_command == "tsserver" { //if default value
                            session.send_config_diagnostic(ConfigDiagnosticAction::EXTEND, &[
                                ConfigDiagnosticMessage {
                                    level: ConfigDiagnosticMessageLevel::WARNING,
                                    message:
                                    "tsserver unable to start. Make sure that tsserver is installed and available in your PATH. You can install it with 'npm install -g typescript@6'. If you want to disable tsserver, set the tsserver_command configuration to an empty string.".to_string(),
                                }
                            ]);
                        } else {
                            session.send_config_diagnostic(ConfigDiagnosticAction::EXTEND, &[
                                ConfigDiagnosticMessage {
                                    level: ConfigDiagnosticMessageLevel::ERROR,
                                    message: format!("Unable to start tsserver with the command: {}. ", session.sync_odoo.config.tsserver_command()),
                                }
                            ]);
                        }
                    }
                },
                Err(error) => {
                    session.send_config_diagnostic(ConfigDiagnosticAction::EXTEND, &[
                    ConfigDiagnosticMessage {
                        level: ConfigDiagnosticMessageLevel::ERROR,
                        message: format!("tsserver ended unexpectedly: {:?}. Most javascript features will not work. See logs for more information", error),
                    }
                ]);},
            }
        }
        session.send_notification("$Odoo/loadingStatusUpdate", "stop");
        session.log_message(MessageType::INFO, format!("End of initialization. Time taken: {} ms", start_time.elapsed().as_millis()));
    }

    /// Start tsserver and configure the external project's `paths` for Odoo's `@addons/*` layout.
    /// Runs on a worker thread (see the call site) so it overlaps the Python build; takes owned
    /// config values because it cannot touch `session`. Ambient `@types`: `core::js_type_files`.
    fn setup_and_start_tsserver(
        tsserver_cmd: String,
        sender_to_main: crossbeam_channel::Sender<ThreadMessage>,
        odoo_path: Option<String>,
        addons_paths: Vec<String>,
        ts_check: bool,
    ) -> Option<TsServerBridge> {
        info!("Starting tsserver with command \"{}\"", tsserver_cmd);
        let mut bridge = match TsServerBridge::new(&tsserver_cmd, sender_to_main, ts_check) {
            Ok(bridge) => bridge,
            Err(err) => {
                warn!("Unable to start tsserver for JS completions: {}", err);
                return None;
            }
        };
        let mut paths: HashMap<String, Vec<String>> = HashMap::default();
        let mut addon_dirs: Vec<PathBuf> = vec![];
        if let Some(ref odoo_path) = odoo_path {
            addon_dirs.push(Path::new(odoo_path).join("addons"));
        }
        for extra in addons_paths.iter() {
            addon_dirs.push(Path::new(extra).to_path_buf());
        }
        for addon_dir in addon_dirs {
            if let Ok(entries) = std::fs::read_dir(&addon_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let src = entry.path().join("static").join("src").join("*");
                        paths.entry(format!("@{}/*", name))
                            .or_default()
                            .push(src.sanitize());
                        if name == "spreadsheet" {
                            let src = entry.path().join("static").join("src").join("index.js");
                            paths.entry(S!("@spreadsheet"))
                                .or_default()
                                .push(src.sanitize());
                            // `@odoo/o-spreadsheet` is deliberately NOT aliased: mirrors
                            // jsconfig.json's exclude of o_spreadsheet.js (a 2.9 MB
                            // minified bundle with no .d.ts that only bloats the program).
                        } else if name == "web" {
                            let path_entry = paths.entry(S!("@odoo/owl")).or_default();
                            path_entry.push(entry.path().join("static").join("src").join("@types").join("owl.d.ts").sanitize());
                            path_entry.push(entry.path().join("static").join("lib").join("owl").join("odoo_module.js").sanitize());
                            let path_entry = paths.entry(S!("@odoo/hoot")).or_default();
                            path_entry.push(entry.path().join("static").join("src").join("@types").join("hoot.d.ts").sanitize());
                            let path_entry = paths.entry(S!("@odoo/hoot-dom")).or_default();
                            path_entry.push(entry.path().join("static").join("lib").join("hoot-dom").join("hoot-dom.ts").sanitize());
                        }
                    }
                }
            }
        }
        bridge.open_external_project(paths);
        Some(bridge)
    }

    pub fn find_stdlib_entry_point(&self) -> Rc<RefCell<EntryPoint>> {
        for entry_point in self.entry_point_mgr.borrow().builtins_entry_points.iter() {
            if entry_point.borrow().path == self.stdlib_dir {
                return entry_point.clone();
            }
        }
        panic!("Unable to find stdlib entry point");
    }

    pub fn load_builtins(session: &mut SessionInfo) -> bool {
        let path = Path::new(&session.sync_odoo.stdlib_dir);
        let builtins_path = path.join("builtins.pyi");
        if !builtins_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find builtins.pyi. Are you sure that typeshed has been downloaded. If you are building from source, make sure to initialize submodules with 'git submodule init' and 'git submodule update'."));
            error!("Unable to find builtins at: {}", builtins_path.sanitize_cow());
            return false;
        };
        let tree_builtins = path.to_tree();
        let entry_stdlib = session.sync_odoo.find_stdlib_entry_point();
        let disk_dir_builtins = session.st().get_symbol(entry_stdlib.borrow().root.into(), tree_builtins.as_slice(), u32::MAX);
        if disk_dir_builtins.is_empty() {
            panic!("Unable to find builtins disk dir symbol");
        }
        let _builtins_rc_symbol = SymbolTable::create_from_path(session, &builtins_path, disk_dir_builtins[0], false);
        BuildScheduler::queue(session, _builtins_rc_symbol.unwrap().unwrap_buildable_key());
        BuildScheduler::process_rebuilds(session, false)
    }

    pub fn build_database(session: &mut SessionInfo) {
        session.log_message(MessageType::INFO, String::from("Building Database"));
        let result = SyncOdoo::build_base(session);
        if result {
            SyncOdoo::build_modules(session);
        }
        if DEBUG_SYMBOL_TABLE_METRICS {
            log_symbol_counts(session.st());
            log_slotmap_capacities(session.st());
            log_memory_usage();
        }
    }

    pub fn read_version(session: &mut SessionInfo, release_path: &Path) -> OdooVersion {
        let mut version = OdooVersion::new(14, 0, 0);
        let release_file = fs::read_to_string(release_path);
        let release_file = match release_file {
            Ok(release_file) => release_file,
            Err(_) => {
                session.log_message(MessageType::INFO, String::from("Unable to read release.py - Aborting"));
                return OdooVersion::default();
            }
        };
        for line in release_file.lines() {
            if line.starts_with("version_info = (") {
                let result = VERSION_REGEX.captures(line);
                match result {
                    Some(result) => {
                        let version_info = result.get(1).unwrap().as_str();
                        let version_info = version_info.split(", ").collect::<Vec<&str>>();
                        let major_str = version_info[0].replace("saas~", "").replace("'", "").replace(r#"""#, "");
                        version = OdooVersion::new(
                            major_str.parse().unwrap(),
                            version_info[1].parse().unwrap(),
                            version_info[2].parse().unwrap(),
                        );
                        break;
                    },
                    None => {
                        session.log_message(MessageType::ERROR, String::from("Unable to detect the Odoo version. Running the tool for the version 14"));
                        break;
                    }
                }
            }
        }
        version
    }

    fn build_base(session: &mut SessionInfo) -> bool {
        let odoo_path = session.sync_odoo.config.odoo_path().clone();
        let Some(odoo_path) = odoo_path.filter(|odoo_path| Path::new(odoo_path).exists()) else {
            info!("Odoo path not provided or is not a valid path. Continuing in single file mode");
            return false;
        };
        session.sync_odoo.has_main_entry = true;
        let odoo_sym = EntryPointMgr::set_main_entry(session, odoo_path.clone());
        let odoo_entry = session.sync_odoo.entry_point_mgr.borrow().main_entry_point.as_ref().unwrap().clone();
        session.sync_odoo.main_entry_tree = odoo_entry.borrow().tree.clone();
        let release_path = Path::new(&odoo_path).join("odoo/release.py");
        let odoo_addon_path = Path::new(&odoo_path).join("addons");
        if !release_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find release.py - Aborting and switching to non-odoo mode"));
            return false;
        }
        let version = SyncOdoo::read_version(session, &release_path);
        if version.major == 0 {
            return false;
        }
        session.log_message(MessageType::INFO, format!("Odoo version: {}", version));
        if version.major < 14 {
            session.log_message(MessageType::ERROR, String::from("Odoo version is less than 14. The tool only supports version 14 and above. Aborting and switching to non-odoo mode"));
            return false;
        }
        session.sync_odoo.version = version;
        //build base
        let config_odoo_path = Path::new(&odoo_path);
        let Some(odoo_sym) = odoo_sym else {
            panic!("Odoo root symbol not found")
        };
        session.st_mut().set_is_external(odoo_sym, false);
        let odoo_odoo = SymbolTable::create_from_path(session, &config_odoo_path.join("odoo"), odoo_sym, false);
        let Some(odoo_odoo) = odoo_odoo else {
            panic!("Not able to find odoo with given path. Aborting...");
        };
        match odoo_odoo {
            SymbolKey::PythonPackage(p) => {
                session.st_mut()[p].self_import = true;
                BuildScheduler::queue(session, odoo_odoo.unwrap_buildable_key());
            },
            SymbolKey::Namespace(_) => {
                //starting from > 18.0, odoo is now a namespace. Start import project from odoo/__main__.py
                let main_file = SymbolTable::create_from_path(session, &config_odoo_path.join("odoo").join("__main__.py"),  odoo_odoo, false);
                let Some(main_file) = main_file else {
                    panic!("Not able to find odoo/__main__.py. Aborting...");
                };
                let f = main_file.unwrap_file_key();
                session.st_mut()[f].self_import = true;
                BuildScheduler::queue(session, main_file.unwrap_buildable_key());
            },
            _ => panic!("Root symbol is not a package or namespace (> 18.0)")
        }
        session.sync_odoo.has_odoo_main_entry = true; // set it now has we need it to parse base addons
        if !BuildScheduler::process_rebuilds(session, false) {
            return false;
        }
        //search common odoo addons path
        let addon_symbols = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons"], &[]), u32::MAX);
        let addon_symbol = if let Some(&SymbolKey::Namespace(addon_ns)) = addon_symbols.first() {
            addon_ns
        } else {
            let odoo = session.sync_odoo.get_symbol(&odoo_path, (&["odoo"], &[]), u32::MAX);
            if odoo.is_empty() {
                session.log_message(MessageType::WARNING, "Odoo not found. Switching to non-odoo mode...".to_string());
                session.sync_odoo.has_odoo_main_entry = false;
                return false;
            }
            //if we are > 18.1, odoo.addons is not imported automatically anymore. Let's try to import it manually
            let addons_folder = SymbolTable::create_from_path(session, &config_odoo_path.join("odoo").join("addons"), odoo_odoo, false);
            if let Some(SymbolKey::Namespace(addons_ns)) = addons_folder {
                addons_ns
            } else {
                session.log_message(MessageType::WARNING, "Not able to find odoo/addons. Please check your configuration. Switching to non-odoo mode...".to_string());
                session.sync_odoo.has_odoo_main_entry = false;
                return false;
            }
        };
        if odoo_addon_path.exists() {
            if session.sync_odoo.load_odoo_addons {
                let path = odoo_addon_path.sanitize();
                session.st_mut()[addon_symbol].add_path(path.clone());
                EntryPointMgr::add_entry_to_addons(session, path,
                    odoo_entry.clone(),
                    vec![Sy!("odoo"), Sy!("addons")]);
            }
        } else {
            session.log_message(MessageType::WARNING, format!("Unable to find odoo addons path at {}. You can ignore this message if you use a nightly build or if your community addons are in another addon paths.", odoo_addon_path.sanitize_cow()));
        }
        for addon in session.sync_odoo.config.addons_paths().clone() {
            let addon_path = Path::new(&addon);
            if addon_path.exists() {
                session.st_mut()[addon_symbol].add_path(addon_path.sanitize());
                EntryPointMgr::add_entry_to_addons(session, addon,
                    odoo_entry.clone(),
                    vec![Sy!("odoo"), Sy!("addons")]);
            }
        }
        true
    }

    fn build_modules(session: &mut SessionInfo) {
        let Some(&SymbolKey::Namespace(addons_symbol)) = session.sync_odoo.get_symbol(
            session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "addons"], &[]), u32::MAX
        ).first() else {
            let message = S!("OdooLS: Unable to find 'odoo/addons'. Check the addons_paths in your config or your file structure. Skipping addons loading...");
            warn!("{}", message);
            session.show_message(MessageType::WARNING, message);
            return;
        };
        let addons_path = session.st()[addons_symbol].paths();
        let mut modules = vec![];
        for addon_path in addons_path.iter() {
            info!("searching modules in {}", addon_path);
            if Path::new(addon_path).exists() {
                //browse all dir in path
                for item in Path::new(addon_path).read_dir().expect("Unable to browse and odoo addon directory") {
                    if let Ok(item) = item
                        && item.file_type().unwrap().is_dir() && !session.sync_odoo.modules.contains_key(item.file_name().to_str().unwrap())
                            && let Some(module_symbol) = SymbolTable::create_module_from_path(session, &item.path(), addons_symbol) {
                                modules.push(module_symbol);
                            }
                }
            }
        }
        let sorted_modules = SyncOdoo::sort_modules(session.st(), modules);
        let n_modules = sorted_modules.len();
        let main_entry = session.sync_odoo.get_main_entry();
        session.sync_odoo.import_cache = Some(ImportCache::default());
        session.sync_odoo.deferred_subfunc_invalidation = Some(FifoWeakHashSet::new());

        // For reporting purposes: progress is split between the two phases of this function: build (arch, arch_eval) and validation.
        const BUILD_PHASE_WEIGHT: u32 = 30; // %
        const VALIDATION_PHASE_WEIGHT: u32 = 100 - BUILD_PHASE_WEIGHT; // %
        let mut reporter = ProgressReporterPercentage::start(session, "Odoo: Indexing modules");

        // Pre-parser: Start reading/parsing files in separate thread.
        let pre_parser = PreParser::new(session, &sorted_modules);
        session.sync_odoo.pre_parse_cache = Some(pre_parser.cache.clone());

        // Build modules (arch + arch_eval)
        for (i, module) in sorted_modules.into_iter().enumerate() {
            // report progress (n_modules > 0, otherwise loop wouldn't run)
            reporter.report_progress(i as u32 * BUILD_PHASE_WEIGHT / n_modules as u32);

            if let Some(mut builder) = PythonArchBuilder::new(session.st(), main_entry.clone(), module.into()) {
                builder.load_arch(session);
            }
            // Drain build queues, skip validation
            while BuildScheduler::build_one(session, &main_entry, false) {
                if session.sync_odoo.terminate_rebuild.load(Ordering::Relaxed) { return; }
            }

            // Update pre_parser on module build progress
            pre_parser.on_module_built(i);
        }
        // Module-build phase done: tear down the pool (joins workers) and drop the cache.
        session.sync_odoo.pre_parse_cache = None;
        drop(pre_parser);

        // Run deferred subfunction invalidations
        if let Some(mut files) = session.sync_odoo.deferred_subfunc_invalidation.take() {
            while let Some(file) = files.pop_front_valid(session.st()) {
                SymbolTable::invalidate_sub_functions(session, file);
            }
        }
        // Drain validation queue
        let total_items = BuildScheduler::validation_queue_len(session) as u32;
        while BuildScheduler::build_one(session, &main_entry, true) {
            if session.sync_odoo.terminate_rebuild.load(Ordering::Relaxed) { return; }
            let items_left = BuildScheduler::validation_queue_len(session) as u32;
            // report progress (total_items > 0, otherwise loop wouldn't run)
            reporter.report_progress(BUILD_PHASE_WEIGHT + (total_items - items_left) * (VALIDATION_PHASE_WEIGHT) / total_items);
        }
        session.sync_odoo.import_cache = None;
        let modules_count = session.sync_odoo.modules.len();
        reporter.end();
        info!("End building modules. {} modules loaded", modules_count);
        session.log_message(MessageType::INFO, format!("End building modules. {} modules loaded", modules_count));
        session.sync_odoo.state_init = InitState::ODOO_READY;
    }

    /// Sort modules by load order
    fn sort_modules(symbol_table: &SymbolTable, modules: Vec<ModuleKey>) -> Vec<ModuleKey> {
        // Build name -> (symbol, dependencies) lookup
        let module_info: HashMap<OYarn, (ModuleKey, Vec<OYarn>)> = modules
            .into_iter()
            .map(|module_key| {
                let (name, depends) = {
                    let module_symbol = &symbol_table[module_key];
                    let name = module_symbol.name.clone();
                    let depends = module_symbol.depends.iter().map(|(d, _)| d.clone()).collect();
                    (name, depends)
                };
                (name, (module_key, depends))
            })
            .collect();

        // Build nodes for graph in (name, dependencies[]) format
        let nodes: Vec<(&str, Vec<&str>)> = module_info
            .iter()
            .map(|(name, (_, deps))| (name.as_str(), deps.iter().map(|s| s.as_str()).collect()))
            .chain([("base", vec![])]) // Include "base" for proper dependency resolution
            .collect();

        let sort_result = sort_by_load_order(nodes);
        debug_assert!(sort_result.sorted.first() == Some(&"base"), "The first module after sorting should be 'base'");

        sort_result.sorted
            .iter()
            .skip(1) // skip "base"
            // TODO: decide what to do with invalid modules. For now, we append them at the end.
            .chain(sort_result.invalid.iter())
            .map(|&name| module_info.get(name).expect("module should exist").0)
            .collect()
    }


    //search for a symbol with a tree local to an unknown entrypoint
    pub fn get_symbol(&self, from_path: &str, tree: TreeStrSlice, position: u32) -> Vec<SymbolKey> {
        //find which entrypoint to use
        for entry in self.entry_point_mgr.borrow().iter_all() {
            let entry_point = entry.borrow();
            if entry_point.is_public() || Path::new(from_path).starts_with(&entry_point.path) {
                let prefix = entry_point.addon_to_odoo_tree.as_ref().unwrap_or(&entry_point.tree);
                let tree_0: Vec<&str> = prefix.iter()
                    .map(|y| y.as_str())
                    .chain(tree.0.iter().copied())
                    .collect();
                let symbols = self.symbol_table.get_symbol(entry_point.root.into(), (&tree_0, tree.1), position);
                if !symbols.is_empty() {
                    return symbols;
                }
            }
        }
        //no valid entry point? that's wrong, an entry shoud have been created
        warn!("Unable to find symbol for entry: {} - tree: {:?}", from_path, tree);
        vec![]
    }

    pub fn get_main_entry(&self) -> Rc<RefCell<EntryPoint>> {
        return self.entry_point_mgr.borrow().main_entry_point.as_ref().expect("Unable to find main entry point").clone()
    }

    /// Ensure that a function symbol's evaluations are as fully populated
    pub fn ensure_func_evaluations(session: &mut SessionInfo, function_key: FunctionKey) {
        let Some(func_file) = session.st().get_file(function_key.into()) else {
            return;
        };
        if session.st()[function_key].evaluations.is_empty() && !session.st().is_external(func_file.into()) {
            // Run Arch eval on file, if possible, then run everything on the fn
            // until arch_eval
            BuildScheduler::build_now(session, func_file, BuildSteps::ARCH_EVAL);
            BuildScheduler::build_now(session, function_key, BuildSteps::ARCH);
            BuildScheduler::build_now(session, function_key, BuildSteps::ARCH_EVAL);
        }
    }

    pub fn is_request_cancelled(&self) -> bool {
        if let Some(request_id) = self.current_request_id.as_ref() {
            return !self.running_request_ids.lock().unwrap().contains(request_id);
        }
        false
    }

    pub fn get_file_mgr(&self) -> Rc<RefCell<FileMgr>> {
        self.file_mgr.clone()
    }

    pub fn unload_path(session: &mut SessionInfo, path: &Path) {
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_all() {
            let path_str = path.sanitize_cow();
            let sym_in_data = entry.borrow().data_symbols.get(path_str.as_ref()).copied();
            if let Some(sym) = sym_in_data {
                if let Some(sym) = sym.upgrade(session.st()) {
                    SymbolTable::unload(session, sym);
                }
                continue;
            }
            let sym_in_js = entry.borrow().js_symbols.get(path_str.as_ref()).cloned();
            if let Some(sym) = sym_in_js {
                if let Some(sym) = sym.upgrade(session.st()) {
                    SymbolTable::unload(session, sym.into());
                }
                continue;
            }
            if entry.borrow().is_valid_for(path) {
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbols = session.st().get_symbol(entry.borrow().root.into(), tree.as_slice(), u32::MAX);
                let Some(&path_symbol) = path_symbols.first() else {
                    continue;
                };
                let Some(source_file) = path_symbol.as_source_file_key() else {
                    continue;
                };
                SymbolTable::unload(session, source_file);
            }
        }
    }

    /// Side effects of unloading a symbol
    pub fn on_unload(session: &mut SessionInfo, symbol: SymbolKey) {
        // Invalidate dependents
        if let Some(source_file) = symbol.as_source_file_key() {
            if DEBUG_MEMORY {
                info!("Unloading symbol {:?} at {:?}", session.st().name(source_file), session.st().path(source_file));
            }
            let buildable_symbol = BuildableSymbolKey::from(source_file);
            SymbolTable::invalidate(session, source_file, session.st().first_step(buildable_symbol));
        }
        match symbol {
            //check if we should not reimport automatically
            SymbolKey::PythonPackage(package_key) => {
                let package = &session.st()[package_key];
                let parent: SymbolKey = package.parent().into();
                if package.self_import {
                    session.sync_odoo.must_reload_paths.push((Wk::from(parent), package.path.clone()));
                }
            },
            SymbolKey::File(file_key) => {
                let file = &session.st()[file_key];
                let parent: SymbolKey = file.parent().into();
                if file.self_import {
                    session.sync_odoo.must_reload_paths.push((Wk::from(parent), file.path.clone()));
                }
            },
            SymbolKey::JsFile(file_key) => {
                let file = &session.st()[file_key];
                if file.self_import {
                    session.sync_odoo.must_reload_paths.push((SymbolKey::from(file.parent()).into(), file.path.clone()));
                }
                let js_file = symbol.as_source_file_key().unwrap();
                ModuleSymbol::on_js_file_unload(session, js_file);
            },
            SymbolKey::Module(module_key) => {
                let module = &session.sync_odoo.symbol_table[module_key];
                session.sync_odoo.modules.remove(&module.dir_name);
            },
            SymbolKey::Class(class_key) => {
                if let Some(model_data) = &session.st()[class_key]._model {
                    let model = session.sync_odoo.models.get(&model_data.name).cloned();
                    if let Some(model) = model {
                        let module = session.st().find_module(class_key);
                        model.borrow_mut().remove_symbol(session, class_key,  module);
                    }
                }
            },
            SymbolKey::XmlFile(_) | SymbolKey::CsvFile(_) => {
                let data_file = symbol.as_source_file_key().unwrap();
                ModuleSymbol::on_data_file_unload(session, data_file);
            },
            _ => {}
        }

    }
    /*
     * Give the symbol that is linked to the given path. As we consider that the file is opened, we do not search in entries that
     * could have it in dependencies but are not the main entry. If not found, create a new entry (is useful if the entry was dropped before
     * due to an inclusion in main entry then removed)
     */
    pub fn get_symbol_of_opened_file(session: &mut SessionInfo, path: &Path) -> Option<SourceFileKey> {
        let path_str = path.sanitize_cow();
        let path_in_tree = path.to_tree_path();
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_main() {
            let sym_in_data = entry.borrow().data_symbols.get(path_str.as_ref()).cloned();
            if let Some(sym) = sym_in_data {
                if let Some(sym) = sym.upgrade(session.st()) {
                    return Some(sym);
                }
                continue;
            }
            if let Some(sym) = entry.borrow().js_symbols.get(path_str.as_ref()) {
                if let Some(sym) = sym.upgrade(session.st()) {
                    return Some(sym.into());
                }
                continue;
            }
            if (entry.borrow().typ == EntryPointType::MAIN || entry.borrow().addon_to_odoo_path.is_some()) && entry.borrow().is_valid_for(path) {
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = session.st().get_symbol(entry.borrow().root.into(), tree.as_slice(), u32::MAX);
                if path_symbol.is_empty() {
                    continue;
                }
                return path_symbol[0].as_source_file_key();
            }
        }
        //Not found? Then return if it is matching a non-public entry strictly matching the file
        let mut found_an_entry = false; //there to ensure that a wrongly built entry would create infinite loop
        for entry in ep_mgr.borrow().custom_entry_points.iter() {
            let sym_in_data = entry.borrow().data_symbols.get(path_str.as_ref()).cloned();
            if let Some(sym) = sym_in_data {
                if let Some(sym) = sym.upgrade(session.st()) {
                    return Some(sym);
                }
                continue;
            }
            let sym_in_js = entry.borrow().js_symbols.get(path_str.as_ref()).cloned();
            if let Some(sym) = sym_in_js {
                if let Some(sym) = sym.upgrade(session.st()) {
                    return Some(sym.into());
                }
                continue;
            }
            if !entry.borrow().is_public() && path_in_tree == Path::new(&entry.borrow().path) {
                found_an_entry = true;
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = session.st().get_symbol(entry.borrow().root.into(), tree.as_slice(), u32::MAX);
                if path_symbol.is_empty() {
                    continue;
                }
                return path_symbol[0].as_source_file_key();
            }
        }
        for entry in ep_mgr.borrow().untitled_entry_points.iter() {
            if entry.borrow().path == path_str {
                let name = path.with_extension("").components().next_back().unwrap().as_os_str().to_str().unwrap().to_string();
                let Some(SymbolKey::File(file)) = session.st()[entry.borrow().root].module_symbols().get(name.as_str()).cloned() else {
                    continue;
                };
                return Some(file.into());
            }
        }
        if !found_an_entry {
            info!("Path {} not found. Creating new entry", path.to_str().expect("unable to stringify path"));
            if EntryPointMgr::create_new_custom_entry_for_path(session, &path_in_tree.sanitize_cow(), &path_str) {
                BuildScheduler::process_rebuilds(session, false);
                return SyncOdoo::get_symbol_of_opened_file(session, path)
            }
        }
        None
    }

    /*
    * Given a path, return a tree that is valid for main entry, transformed by relational entries if necessary
     */
    pub fn path_to_main_entry_tree(&self, path: &Path) -> Option<Tree> {
        for entry in self.entry_point_mgr.borrow().iter_main() {
            if (entry.borrow().typ == EntryPointType::MAIN || entry.borrow().addon_to_odoo_path.is_some()) && entry.borrow().is_valid_for(path) {
                let tree = entry.borrow().get_tree_for_entry(path);
                return Some(tree);
            }
        }
        None
    }

    pub fn get_main_entry_tree(&self, symbol_key: impl Into<SymbolKey>) -> Tree {
        let symbol_key = symbol_key.into();
        let mut tree = self.symbol_table.get_tree(symbol_key);
        let odoo_tree = &self.main_entry_tree;
        if tree.0.starts_with(odoo_tree) {
            tree.0.drain(0..odoo_tree.len());
        }
        tree
    }

    pub fn match_tree_from_any_entry(&self, symbol_key: SymbolKey, tree: TreeStrSlice) -> bool {
        let symbol_table = &self.symbol_table;
        let (mut self_tree, entry) = symbol_table.get_tree_and_entry(symbol_key);
        'outer: for entry in self.entry_point_mgr.borrow().iter_for_import(&entry) {
            if entry.borrow().tree.len() > self_tree.0.len() {
                continue;
            }
            for (index, tree_el) in entry.borrow().tree.iter().enumerate() {
                if self_tree.0[index] != *tree_el {
                    continue 'outer;
                }
            }
            return Tree(self_tree.0.split_off(entry.borrow().tree.len()), self_tree.1) == tree;
        }
        false
    }

    pub fn is_in_workspace_or_entry(session: &SessionInfo, path: &str) -> bool {
        if session.sync_odoo.file_mgr.borrow().is_in_workspace(path) {
            return true;
        }
        for entry in session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter() {
            let entry = entry.borrow();
            if path == entry.path {
                return true
            }
        }
        false
    }

    pub fn is_in_main_entry(session: &mut SessionInfo, path: &[OYarn]) -> bool{
        path.starts_with(session.sync_odoo.main_entry_tree.as_slice())
    }

    fn is_non_main_manifest_file(symbol_table: &SymbolTable, file_symbol: SourceFileKey, file_path_buff: &Path) -> bool {
        !symbol_table.get_entry(file_symbol).borrow().is_main()
        && file_path_buff.components().next_back()
            .is_some_and(|c| c.as_os_str().to_str().is_some_and(|s| s == "__manifest__.py"))
    }

    pub fn load_capabilities(&mut self, capabilities: &lsp_types::ClientCapabilities) {
        info!("Client capabilities: {:?}", capabilities);
        self.capabilities = capabilities.clone();
        self.calculate_encoding();
    }

    fn calculate_encoding(&mut self) {
        let maybe_client_encoding = self.capabilities
        .general
        .as_ref()
        .and_then(|general_capabilities| general_capabilities.position_encodings.as_ref())
        .and_then(|encodings| {
            encodings
                .iter()
                .filter_map(|encoding| {
                    if encoding == &PositionEncodingKind::UTF8 {
                        Some((2, PositionEncoding::Utf8))
                    } else if encoding == &PositionEncodingKind::UTF16 {
                        Some((0, PositionEncoding::Utf16))
                    } else if encoding == &PositionEncodingKind::UTF32 {
                        Some((1, PositionEncoding::Utf32))
                    } else {
                        None
                    }
                })
                .max_by_key(|(ord, _)| *ord) // this selects the highest priority position encoding
                // Order is UTF8 > UTF32 > UTF16
                // Because Ruff prefers UTF8, UTF32 has constant size, and UTF16 is the default and is mandatory
        });
        if let Some((_, encoding)) = maybe_client_encoding {
            self.encoding = encoding;
        }
    }

    /**
     * search for an xml_id in the already registered xml files.
     * */
    pub fn get_xml_ids(session: &mut SessionInfo, from_file: SourceFileKey, xml_id: &str, range: &std::ops::Range<usize>, diagnostics: &mut Vec<Diagnostic>) -> WeakSet<XmlId> {
        if !session.st().get_entry(from_file).borrow().is_main() {
            return WeakSet::new();
        }
        let id_split = xml_id.split(".").collect::<Vec<&str>>();
        let mut module = None;
        if id_split.len() == 1 {
            // If no module name, we are in the current module
            module = session.st().find_module(from_file);
        } else if id_split.len() == 2 {
            // Try to find the module by name
            if let Some(&m) = session.sync_odoo.modules.get(*id_split.first().unwrap()) {
                module = m.upgrade(session.st());
            }
        } else if id_split.len() > 2 {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[xml_id]) {
                diagnostics.push(lsp_types::Diagnostic {
                    range: lsp_types::Range {
                        start: lsp_types::Position::new(range.start as u32, 0),
                        end: lsp_types::Position::new(range.end as u32, 0),
                    },
                    ..diagnostic.clone()
                });
            }
            return WeakSet::new();
        }
        let Some(module_key) = module else {
            return WeakSet::new();
        };
        ModuleSymbol::get_xml_id(session.st(), module_key, id_split.last().unwrap()).unwrap_or_default()
    }

    pub fn get_ts_dict(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.dict.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.dict = self.get_symbol("", (&["builtins"], &["dict"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.dict
    }

    pub fn get_ts_tuple(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.tuple.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.tuple = self.get_symbol("", (&["builtins"], &["tuple"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.tuple
    }

    pub fn get_ts_set(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.set.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.set = self.get_symbol("", (&["builtins"], &["set"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.set
    }

    pub fn get_ts_list(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.list.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.list = self.get_symbol("", (&["builtins"], &["list"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.list
    }

    pub fn get_ts_string(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.string.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.string = self.get_symbol("", (&["builtins"], &["str"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.string
    }

    pub fn get_ts_boolean(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.boolean.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.boolean = self.get_symbol("", (&["builtins"], &["bool"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.boolean
    }

    pub fn get_ts_int(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.int.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.int = self.get_symbol("", (&["builtins"], &["int"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.int
    }

    pub fn get_ts_float(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.float.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.float = self.get_symbol("", (&["builtins"], &["float"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.float
    }

    pub fn get_ts_complex(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.complex.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.complex = self.get_symbol("", (&["builtins"], &["complex"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.complex
    }

    pub fn get_ts_ellipsis(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.ellipsis.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.ellipsis = self.get_symbol("", (&["builtins"], &["Ellipsis"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.ellipsis
    }

    pub fn get_ts_bytes(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.bytes.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.bytes = self.get_symbol("", (&["builtins"], &["bytes"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.bytes
    }

    pub fn get_ts_object(&mut self) -> Wk<SymbolKey> {
        if self.typeshed_weak_cache.object.is_expired(&self.symbol_table) {
            self.typeshed_weak_cache.object = self.get_symbol("", (&["builtins"], &["object"]), u32::MAX).last().copied().unwrap().into();
        }
        self.typeshed_weak_cache.object
    }

    /// Add a language code from a source of res_lang records.
    pub fn add_language(session: &mut SessionInfo, lang_code: &str, source_file: SourceFileKey) {
        let languages = session.sync_odoo.languages_by_source
            .entry(source_file)
            .or_default();
        languages.extend(expand_language_code(lang_code));
        SyncOdoo::revalidate_language_dependents(session);
    }

    /// Remove a source of res_lang records from the language registry.
    pub fn remove_language_source(session: &mut SessionInfo, source_file: SourceFileKey) {
        if session.sync_odoo.languages_by_source.remove(&source_file).is_some() {
            SyncOdoo::revalidate_language_dependents(session);
        }
    }

    /// Check if a language code exists and register the dependent symbol.
    pub fn check_language_and_track(&mut self, lang: &str, dependent: SymbolKey) -> bool {
        self.language_dependents.insert(dependent);
        self.languages_by_source.iter_valid_values(&self.symbol_table).any(|langs| langs.contains(lang))
            || self.config.additional_languages().contains(lang)
    }

    /// For testing purposes only. Use `check_language_and_track` for actual
    /// language checks.
    pub fn _get_languages(&mut self) -> HashSet<String> {
        self.languages_by_source
            .iter_valid_values(&self.symbol_table)
            .flat_map(|langs| langs.iter().cloned())
            .chain(self.config.additional_languages().iter().cloned())
            .collect()
    }

    /// Schedule revalidation for all language-dependent symbols.
    pub(crate) fn revalidate_language_dependents(session: &mut SessionInfo) {
        let to_revalidate = session.sync_odoo.language_dependents.drain_valid(&session.sync_odoo.symbol_table);
        for sym in to_revalidate {
            SymbolTable::invalidate(session, sym.as_source_file_key().unwrap(), BuildSteps::VALIDATION);
            BuildScheduler::queue(session, sym.unwrap_buildable_key());
        }
    }

    /// Cached version of `is_file_cs` that uses the import cache if available.
    /// Avoids multiple stat syscalls for the same file during a rebuild.
    pub fn is_file_cs(&mut self, path: &str) -> bool {
        let Some(cache) = self.import_cache.as_mut().map(|c| &mut c.is_file_cs) else {
            return is_file_cs(path);
        };
        *cache.entry(path.to_string()).or_insert_with(|| is_file_cs(path))
    }

    /// Cached version of `is_dir_cs` that uses the import cache if available.
    /// Avoids multiple stat syscalls for the same dir during a rebuild.
    pub fn is_dir_cs(&mut self, path: &str) -> bool {
        let Some(cache) = self.import_cache.as_mut().map(|c| &mut c.is_dir_cs) else {
            return is_dir_cs(path);
        };
        *cache.entry(path.to_string()).or_insert_with(|| is_dir_cs(path))
    }

    /// Take the prepared payload (Python AST or XML/CSV data file) for
    /// `sanitized_path`, if a [`crate::core::pre_parser`] worker produced one
    /// ahead of the build.
    pub fn take_preloaded(session: &SessionInfo, sanitized_path: &str) -> Option<PreloadedFile> {
        session.sync_odoo.pre_parse_cache.as_ref()?.take(sanitized_path)
    }

    pub fn pre_parse_cache(&self) -> Option<&Arc<PreParseCache>> {
        self.pre_parse_cache.as_ref()
    }
}

#[derive(Debug)]
pub struct Odoo {}

impl Odoo {

    pub fn read_selected_configuration(session: &mut SessionInfo) -> Result<Option<String>, String> {
        let configuration_item = ConfigurationItem {
            scope_uri: None,
            section: Some("Odoo".to_string()),
        };
        let config_params = ConfigurationParams {
            items: vec![configuration_item],
        };
        let config = match session.send_request::<ConfigurationParams, Vec<serde_json::Value>>(WorkspaceConfiguration::METHOD, config_params) {
            Ok(Some(config)) => config,
            Ok(None) => {
                return Err(S!("Read empty config from client response, please try again"));
            }
            Err(_) => {
                return Err(S!("Unable to get configuration from client, client not available"));
            }
        };
        let config = config.first();
        if config.is_none() {
            session.log_message(MessageType::ERROR, String::from("No config found for Odoo. Exiting..."));
            return Err(S!("No config found for Odoo"));
        }
        let value = config
            .and_then(|c| c.as_object())
            .and_then(|c| c.get("selectedProfile"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        Ok(value)
    }

    pub fn send_all_configurations(session: &mut SessionInfo) {
        if let Some(ref config_file) = session.sync_odoo.config_file {
            let mut configs_map = serde_json::Map::new();
            for entry in config_file.entries() {
                let html = crate::core::config::ConfigView::single(entry.clone()).to_html_string();
                configs_map.insert(entry.name.clone(), serde_json::Value::String(html));
            }

            configs_map.insert(
                "__all__".to_string(),
                serde_json::Value::String(config_file.to_html_string())
            );
            // Send the HTML map, the config file as JSON, and the rejected values (diagnostics).
            let payload = serde_json::json!({
                "html": serde_json::Value::Object(configs_map),
                "configFile": config_file,
                "diagnostics": config_file.diagnostic_messages(),
            });
            session.send_notification(
                "$Odoo/setConfiguration",
                payload
            );
        }
    }

    pub fn init(session: &mut SessionInfo) {
        let start = std::time::Instant::now();
        session.log_message(MessageType::LOG, String::from("Building new Odoo knowledge database"));
        let config = get_configuration(session);
        if let Ok((_, config_file)) = &config {
            session.sync_odoo.config_file = Some(config_file.clone());
            Odoo::send_all_configurations(session);
        }
        let maybe_selected_config = match Odoo::read_selected_configuration(session){
            Ok(config) => config,
            Err(e) => {
                session.show_message(MessageType::ERROR, format!("Unable to read selected configuration: {}  \n\nPlease select a correct profile or fix the issues in the config", e));
                error!(e);
                return;
            }
        };
        let selected_config = match session.sync_odoo.selected_config {
            Some(ref current) => {
                current.clone()
            },
            None => {
                // If no configuration selected by cli arguments, we try to get it from client, if not found we use default
                let selected_config = match maybe_selected_config {
                    None => DEFAULT_PROFILE_NAME.to_string(),
                    Some(c) if c.is_empty() => DEFAULT_PROFILE_NAME.to_string(),
                    Some(config) => config,
                };
                info!("Selected config profile from client : ({})", selected_config);
                selected_config
            }
        };
        info!("Selected config profile ({})", selected_config);
        session.sync_odoo.selected_config = Some(selected_config.clone());
        if selected_config == "Disabled" {
            info!("OdooLS is disabled. Exiting...");
            return;
        }
        let config = config.and_then(|(ce, _)|{
            ce.get(&selected_config).cloned().ok_or(format!("Unable to find selected configuration \"{}\"", selected_config))
        });
        match config {
            Ok(config) => {
                if config.is_abstract() {
                    session.show_message(MessageType::ERROR, format!("Selected configuration ({}) is abstract. Please select a valid configuration and restart.", config.name));
                    return;
                }
                session.update_delay_thread_delay_duration(config.auto_refresh_delay());
                SyncOdoo::init(session, config);
                session.log_message(MessageType::LOG, format!("End building database in {} seconds. {} detected modules.",
                    (std::time::Instant::now() - start).as_secs(),
                    session.sync_odoo.modules.len()))
            },
            Err(e) => {
                session.show_message(MessageType::ERROR, format!("Unable to load config: {}  \n\nPlease select a correct profile or fix the issues in the config", e));
                // The popup is transient; also record it as a persistent diagnostic so
                // it stays visible in the status bar/tooltip until the config is fixed.
                session.send_config_diagnostic(ConfigDiagnosticAction::EXTEND, &[
                    ConfigDiagnosticMessage {
                        level: ConfigDiagnosticMessageLevel::ERROR,
                        message: format!("Unable to load config: {e}"),
                    }
                ]);
                error!(e);
            }
        }
    }

    pub fn register_capabilities(session: &mut SessionInfo) {
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![
                FileSystemWatcher {
                    glob_pattern: GlobPattern::String("**/*.{py,pyi,xml,csv,js,ts}".to_string()),
                    kind: Some(WatchKind::Change | WatchKind::Create | WatchKind::Delete),
                },
            ],
        };
        let text_document_change_registration_options = TextDocumentChangeRegistrationOptions {
            document_selector: None,
            sync_kind: TextDocumentSyncKind::INCREMENTAL
        };
        let registrations = vec![
            Registration {
                id: "workspace/didChangeWatchedFiles".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: Some(serde_json::to_value(options).unwrap()),
            },
            Registration {
                id: "workspace/didChangeConfiguration".to_string(),
                method: "workspace/didChangeConfiguration".to_string(),
                register_options: None,
            },
            Registration {
                id: "textDocument/didOpen".to_string(),
                method: "textDocument/didOpen".to_string(),
                register_options: None,
            },
            Registration {
                id: "textDocument/didChange".to_string(),
                method: "textDocument/didChange".to_string(),
                register_options: Some(serde_json::to_value(text_document_change_registration_options).unwrap()),
            },
            Registration {
                id: "textDocument/didClose".to_string(),
                method: "textDocument/didClose".to_string(),
                register_options: None,
            }
        ];
        let params = RegistrationParams{
            registrations
        };
        let result = session.send_request::<RegistrationParams, ()>(RegisterCapability::METHOD, params);
        if let Err(e) = result {
            panic!("Capabilities registration went wrong: {:?}", e);
        }
        info!("Registered Capabilities");
    }

    pub fn handle_hover(session: &mut SessionInfo, params: HoverParams) -> Result<Option<Hover>, ResponseError> {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("Hover requested on {} at {} - {}",
            *params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character));
        let path = match params.text_document_position_params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document_position_params.text_document.uri.to_string();
                if [".py", ".pyi", ".xml", ".csv", ".js", ".ts"].iter().all(|ext| !uri.ends_with(ext)) {
                    return Ok(None);
                }
                match params.text_document_position_params.text_document.uri.to_file_path(){
                    Ok(path) => path.sanitize(),
                    Err(error) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}: {}", *params.text_document_position_params.text_document.uri, error),
                            data: None,
                        }
                    ),
                }
            },
            Some(schema) if schema == "untitled" => params.text_document_position_params.text_document.uri.to_string(),
            _ => return Ok(None),
        };
        let file_path_buf = Path::new(&path);
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, file_path_buf) {
            if SyncOdoo::is_non_main_manifest_file(session.st(), file_symbol, file_path_buf) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
            if let Some(file_info) = file_info {
                if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                let ast_type = file_info.borrow().file_info_ast.borrow().ast.clone();
                match ast_type {
                    Ast::PythonAst(_) => {
                        if file_info.borrow_mut().file_info_ast.borrow().ast.as_py_ast().indexed_module.is_some() {
                            return Ok(HoverFeature::hover_python(session, file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                        }
                    },
                    Ast::XmlAst => {
                        let Position { line, character } = params.text_document_position_params.position;
                        // OWL-template JS expressions first; everything else → XML hover.
                        // @todo: check if not breaking python-related hover
                        if let Some(hover) = owl_virtual::hover_xml_owl(session, &file_info, line, character) {
                            return Ok(Some(hover));
                        }
                        return Ok(HoverFeature::hover_xml(session, file_symbol, &file_info, line, character));
                    },
                    Ast::CsvAst => {
                        return Ok(HoverFeature::hover_csv(session, file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    },
                    Ast::JsAst(_) => {
                        return Ok(HoverFeature::hover_js(session, &file_info.borrow().uri, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn handle_semantic_tokens(session: &mut SessionInfo, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>, ResponseError> {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        let path = match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document.uri.as_str();
                if [".py", ".js", ".ts", ".xml"].iter().all(|&ext| !uri.ends_with(ext) ) {
                    return Ok(None);
                }
                match params.text_document.uri.to_file_path() {
                    Ok(path) => path.sanitize(),
                    Err(_) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}", uri),
                            data: None,
                        }
                    ),
                }
            },
            _ => return Ok(None),
        };
        let file_path_buf = PathBuf::from(path.clone());
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &file_path_buf) {
            let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
            if let Some(file_info) = file_info {
                if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                let ast_type = file_info.borrow().file_info_ast.borrow().ast.clone();
                match ast_type {
                    Ast::PythonAst(_) => {
                        if file_info.borrow().file_info_ast.borrow().ast.as_py_ast().indexed_module.is_some() {
                            let tokens = SemanticTokensFeature::tokens_python(session, file_symbol, &file_info);
                            return Ok(Some(SemanticTokensResult::Tokens(tokens)));
                        }
                    },
                    Ast::JsAst(_) => {
                        let uri = file_info.borrow().uri.clone();
                        let tokens = SemanticTokensFeature::tokens_javascript(session, &uri, &file_info);
                        return Ok(Some(SemanticTokensResult::Tokens(tokens)));
                    },
                    Ast::XmlAst => {
                        if let Some(tokens) = owl_virtual::semantic_tokens_xml(session, &file_info) {
                            return Ok(Some(SemanticTokensResult::Tokens(tokens)));
                        }
                    },
                    _ => {},
                }
            }
        }
        Ok(None)
    }

    pub fn handle_goto_definition(session: &mut SessionInfo, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>, ResponseError> {
        Odoo::handle_gotos(session, params, false)
    }

    pub fn handle_goto_declaration(session: &mut SessionInfo, params: GotoDefinitionParams) -> Result<Option<GotoDeclarationResponse>, ResponseError> {
        Odoo::handle_gotos(session, params, true)
    }

    fn handle_gotos(session: &mut SessionInfo, params: GotoDefinitionParams, is_declaration: bool) -> Result<Option<GotoDeclarationResponse>, ResponseError> {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("{} requested on {} at {} - {}",
            match is_declaration {
                false => "GoToDefinition",
                true => "GoToDeclaration"
            },
            *params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character));
        let path = match params.text_document_position_params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document_position_params.text_document.uri.to_string();
                if [".py", ".pyi", ".xml", ".csv", ".js", ".ts"].iter().all(|ext| !uri.ends_with(ext)) {
                    return Ok(None);
                }
                match params.text_document_position_params.text_document.uri.to_file_path(){
                    Ok(path) => path.sanitize(),
                    Err(error) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}: {}", *params.text_document_position_params.text_document.uri, error),
                            data: None,
                        }
                    ),
                }
            },
            Some(schema) if schema == "untitled" => params.text_document_position_params.text_document.uri.to_string(),
            _ => return Ok(None),
        };
        let file_path_buf = Path::new(&path);
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, file_path_buf) {
            if SyncOdoo::is_non_main_manifest_file(session.st(), file_symbol, file_path_buf) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
            if let Some(file_info) = file_info {
                if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                    return Ok(None);
                }
                return match is_declaration {
                    false => {
                        Ok(DefinitionFeature::get_location(session, file_symbol, &file_info,
                            params.text_document_position_params.position.line,
                            params.text_document_position_params.position.character))
                    },
                    true => {
                        Ok(DeclarationFeature::get_location(session, file_symbol, &file_info,
                            params.text_document_position_params.position.line,
                            params.text_document_position_params.position.character))
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn handle_references(session: &mut SessionInfo, params: ReferenceParams) -> Result<Option<Vec<Location>>, ResponseError> {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("References requested on {} at {} - {}",
            *params.text_document_position.text_document.uri,
            params.text_document_position.position.line,
            params.text_document_position.position.character));
        let uri = params.text_document_position.text_document.uri.to_string();
        let path = FileMgr::uri2pathname(uri.as_str());
        let file_path = Path::new(&path);
        if [".py", ".pyi", ".xml", ".csv", ".js", ".ts"].iter().any(|ext| uri.ends_with(ext))
            && let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, file_path)
        {
            if SyncOdoo::is_non_main_manifest_file(session.st(), file_symbol, file_path) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
            if let Some(file_info) = file_info {
                if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                return Ok(ReferenceFeature::get_references(session, file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character));
            }
        }
        Ok(None)
    }

    pub fn handle_autocomplete(session: &mut SessionInfo ,params: CompletionParams) -> Result<Option<CompletionResponse>, ResponseError> {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("Completion requested at {}:{}-{}",
            params.text_document_position.text_document.uri.as_str(),
            params.text_document_position.position.line,
            params.text_document_position.position.character
            ));
        let (schema, path) = match params.text_document_position.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document_position.text_document.uri.to_string();
                if [".py", ".pyi", ".xml", ".csv", ".js", ".ts"].iter().all(|ext| !uri.ends_with(ext)) {
                    return Ok(None);
                }
                match params.text_document_position.text_document.uri.to_file_path(){
                    Ok(path) => (schema, path.sanitize()),
                    Err(error) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}: {}", *params.text_document_position.text_document.uri, error),
                            data: None,
                        }
                    ),
                }
            },
            Some(schema) if schema == "untitled" => (schema, params.text_document_position.text_document.uri.to_string()),
            _ => return Ok(None)
        };
        let path_buf = Path::new(&path);
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, path_buf) {
            if SyncOdoo::is_non_main_manifest_file(session.st(), file_symbol, path_buf) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
            if let Some(file_info) = file_info {
                if schema != "untitled" && !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                let ast_type = file_info.borrow().file_info_ast.borrow().ast.clone();
                match ast_type {
                    Ast::JsAst(_) => {
                        if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                            let items = bridge.completion_items_for_content(
                                &path,
                                params.text_document_position.position.line,
                                params.text_document_position.position.character,
                            );
                            return Ok(Some(CompletionResponse::Array(items)));
                        }
                        return Ok(None);
                    },
                    Ast::XmlAst => {
                        if let Some(items) = owl_virtual::completion_xml_owl(session, &file_info, params.text_document_position.position.line, params.text_document_position.position.character) {
                            return Ok(Some(CompletionResponse::Array(items)));
                        }
                    },
                    _ => {}
                }
                if matches!(file_info.borrow_mut().file_info_ast.borrow().ast, Ast::PythonAst(_)) && file_info.borrow_mut().file_info_ast.borrow().ast.as_py_ast().indexed_module.is_some() {
                    return Ok(CompletionFeature::autocomplete(session,
                        file_symbol,
                        &file_info,
                        params.context,
                        params.text_document_position.position.line,
                        params.text_document_position.position.character
                    ));
                }
            }
        }
        Ok(None)
    }

    /// Fill in the deferred half of a JS/OWL completion item: its signature and docs — and,
    /// for auto-imports, the `import …;` edit — come back only from `completionEntryDetails`.
    /// Items carrying no resolve data (Python's) resolve to themselves.
    pub fn handle_completion_resolve(session: &mut SessionInfo, mut item: CompletionItem) -> Result<CompletionItem, ResponseError> {
        // Consume the data: it identified the entry for this round trip only.
        let Some(data) = item.data.take() else {
            return Ok(item);
        };
        let resolve: TsCompletionResolveData = match serde_json::from_value(data) {
            Ok(resolve) => resolve,
            Err(err) => {
                warn!("Unrecognized completion item data, resolving as-is: {}", err);
                return Ok(item);
            }
        };
        let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() else {
            return Ok(item);
        };
        let Some(details) = bridge.completion_entry_details(
            &resolve.file,
            resolve.line,
            resolve.character,
            &resolve.name,
            resolve.source.as_deref(),
            resolve.data.as_ref(),
        ) else {
            return Ok(item);
        };

        item.detail = details.detail;
        item.documentation = details.documentation.map(Documentation::String);
        // A virtual-doc item's edits are in coordinates the client never saw — never apply them.
        if !details.additional_text_edits.is_empty()
            && !owl_virtual::is_owl_artifact_path(&resolve.file) {
            item.additional_text_edits = Some(details.additional_text_edits);
        }
        Ok(item)
    }

    pub fn handle_did_change_configuration(_session: &mut SessionInfo, _params: DidChangeConfigurationParams) {
    }

    pub fn handle_did_change_workspace_folders(session: &mut SessionInfo, params: DidChangeWorkspaceFoldersParams) {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let mut file_mgr = file_mgr.borrow_mut();
        for added in params.event.added {
            file_mgr.add_workspace_folder(added.name.clone(), added.uri);
        }
        for removed in params.event.removed {
            file_mgr.remove_workspace_folder(removed.name.clone(), removed.uri);
        }
    }

    pub fn handle_did_change_watched_files(session: &mut SessionInfo, params: DidChangeWatchedFilesParams) {
        let mut to_create = vec![];
        let mut to_delete = vec![];
        let mut to_change = vec![];
        for event in params.changes {
            if event.uri.to_string().contains(".git") {
                continue;
            }
            match event.typ {
                FileChangeType::CREATED  => { to_create.push(FileCreate{uri: event.uri.to_string()}); }
                FileChangeType::DELETED => { to_delete.push(FileDelete{uri: event.uri.to_string()}); }
                FileChangeType::CHANGED => {
                    to_change.push(event.uri);
                }
                _ => { panic!("Invalid File Change Event Type: {:?}", event);}
            }
        }
        if !to_create.is_empty() {
            Odoo::handle_did_create(session, CreateFilesParams {
                files: to_create
            });
        }
        if !to_delete.is_empty() {
            Odoo::handle_did_delete(session, DeleteFilesParams {
                files: to_delete
            });
        }
        if !to_change.is_empty() {
            Odoo::handle_file_update(session, &to_change);
        }
    }

    fn handle_file_update(session: &mut SessionInfo, file_uris: &[Uri]) {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for uri in file_uris.iter() {
            let path = match uri.to_file_path() {
                Ok(path) => path,
                Err(error) => {
                    let msg = format!("Invalid file URI: {}: {}", **uri, error);
                    session.log_message(MessageType::ERROR, msg.clone());
                    warn!("{}", &msg);
                    continue;
                }
            };
            if Odoo::check_handle_config_file_update(session, &path) {
                continue; //config file update, handled by the config file handler
            }
            session.log_message(MessageType::INFO, format!("File update: {}", path.sanitize_cow()));
            let file_extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let (valid, updated) = Odoo::update_file_cache(session, &path.sanitize_cow(), file_extension, None, -100);
            if valid && updated {
                Odoo::update_file_index(session, &path, file_extension, false, true);
            }
        }
    }

    pub fn handle_did_open(session: &mut SessionInfo, params: DidOpenTextDocumentParams) {
        //to implement Incremental update of file caches, we have to handle DidOpen notification, to be sure
        // that we use the same base version of the file for future incrementation.
        match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                match params.text_document.uri.to_file_path(){
                    Ok(path) => {
                        let sanitized_path = path.sanitize_cow();
                        session.log_message(MessageType::INFO, format!("File opened: {}", sanitized_path));
                        let file_extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                        if ["js", "ts"].contains(&file_extension) && session.sync_odoo.tsserver_bridge.is_some() {
                            // Staged first, so the send below already carries the declarations.
                            let type_files = js_type_files::type_files_for(session, &sanitized_path);
                            if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                                bridge.stage_type_files(&type_files);
                                bridge.open_file(&sanitized_path, &params.text_document.text);
                                // `open_file` only sends when the file is a new root; covers re-opens.
                                bridge.commit_transient_roots();
                            }
                        }
                        let (valid, updated) = Odoo::update_file_cache(session, &sanitized_path, file_extension, Some(&[TextDocumentContentChangeEvent{
                            range: None,
                            range_length: None,
                            text: params.text_document.text
                        }]), params.text_document.version);
                        if valid {
                            session.sync_odoo.opened_files.push(sanitized_path.to_string());
                            if session.sync_odoo.state_init == InitState::NOT_READY {
                                return
                            }
                            let tree = session.sync_odoo.path_to_main_entry_tree(&path);
                            let tree_path = path.to_tree_path();
                            if tree.is_none() ||
                            (session.st().get_symbol(session.sync_odoo.get_main_entry().borrow().root.into(), tree.as_ref().unwrap().as_slice(), u32::MAX).is_empty()
                            && !session.sync_odoo.get_main_entry().borrow().data_symbols.contains_key(sanitized_path.as_ref())
                            && !session.sync_odoo.get_main_entry().borrow().js_symbols.contains_key(sanitized_path.as_ref()))
                            {
                                //main entry doesn't handle this file. Let's test customs entries, or create a new one
                                let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
                                for custom_entry in ep_mgr.borrow().custom_entry_points.iter() {
                                    if custom_entry.borrow().path == tree_path.sanitize_cow() {
                                        if updated{
                                            Odoo::update_file_index(session, &path, file_extension, true, false);
                                        }
                                        return;
                                    }
                                }
                                EntryPointMgr::create_new_custom_entry_for_path(session, &tree_path.sanitize_cow(), &sanitized_path);
                                BuildScheduler::process_rebuilds(session, false);
                            } else if updated {
                                Odoo::update_file_index(session, &path, file_extension, true, false);
                            }
                        }
                    },
                    Err(error) => {
                        let msg = format!("Invalid file URI: {}: {}", *params.text_document.uri, error);
                        session.log_message(MessageType::ERROR, msg.clone());
                        session.show_message(MessageType::ERROR, msg.clone());
                        warn!("{}", &msg);
                    }
                }
            },
            Some(schema) if schema == "untitled" => {
                if !["python"].contains(&params.text_document.language_id.as_str()) {
                    return; // We only handle python temporary files
                }
                let path = params.text_document.uri.to_string(); // In VSCode it is Untitled-N
                let (valid, updated) = Odoo::update_file_cache(session, &path, "py", Some(&[TextDocumentContentChangeEvent{
                    range: None,
                    range_length: None,
                    text: params.text_document.text
                }]), params.text_document.version);
                if valid {
                    session.sync_odoo.opened_files.push(path.clone());
                    if session.sync_odoo.state_init == InitState::NOT_READY {
                        return
                    }
                    if updated {
                        SyncOdoo::unload_path(session, Path::new(&path));
                    }
                }
                EntryPointMgr::create_new_untitled_entry_for_path(session, &path);
                BuildScheduler::process_rebuilds(session, false);
            }, // temporary file
            Some(scheme) => {
                warn!("Unsupported URI scheme: {}", scheme);
            },
            None => {
                warn!("No URI scheme found");
            }
        }
    }

    pub fn handle_did_close(session: &mut SessionInfo, params: DidCloseTextDocumentParams) {
        let path = match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                match params.text_document.uri.to_file_path().map(|path_buf| path_buf.sanitize()){
                    Ok(path) => path,
                    Err(error) => {
                        warn!("Invalid file URI: {}: {}", params.text_document.uri.to_string(), error);
                        return;
                    }
                }
            },
            Some(schema) if schema == "untitled" =>params.text_document.uri.to_string(),
            Some(scheme) => {
                warn!("Unsupported URI scheme: {}", scheme);
                return;
            },
            None => {
                warn!("No URI scheme found");
                return;
            }
        };
        session.log_message(MessageType::INFO, format!("File closed: {path}"));
        session.sync_odoo.opened_files.retain(|x| x != &path);
        if (path.ends_with(".js") || path.ends_with(".ts"))
            && let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                bridge.close_file(&path);
            }
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
        if let Some(file_info) = file_info {
            file_info.borrow_mut().opened = false;
            file_info.borrow_mut().version = None;
        }
        session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&mut session.sync_odoo.symbol_table, &Path::new(&path).to_tree_path().sanitize_cow());
    }

    pub fn search_symbols_to_rebuild(session: &mut SessionInfo, path: &str) {
        let path_for_tree = Path::new(path).to_tree_path();
        //search if the path does match a missing file path somewhere
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        let tree = session.sync_odoo.path_to_main_entry_tree(Path::new(path));
        if let Some(tree) = tree
            && let Some(main) = ep_mgr.borrow().main_entry_point.as_ref() {
                main.borrow_mut().search_symbols_to_rebuild(session, path, tree);
            }
        for entry in ep_mgr.borrow().iter_all_but_main() {
            if entry.borrow().is_valid_for(Path::new(path)) {
                let tree = entry.borrow().get_tree_for_entry(Path::new(path));
                entry.borrow_mut().search_symbols_to_rebuild(session, path, tree);
            }
        }
        //test if the new path is a new module
        if let Some(parent_path) = path_for_tree.parent() {
            let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
            for entry in ep_mgr.borrow().addons_entry_points.iter() {
                if entry.borrow().path == parent_path.sanitize_cow() {
                    if let SymbolKey::Namespace(addons) = entry.borrow().get_symbol(session.st()).unwrap()
                    && let Some(module_symbol) = SymbolTable::create_module_from_path(session, &path_for_tree, addons) {
                        BuildScheduler::queue(session, BuildableSymbolKey::Module(module_symbol));
                    }
                    break;
                }
            }
            if parent_path.sanitize() == session.sync_odoo.config.odoo_path().as_deref().unwrap_or_default().to_string() + "/odoo/addons" {
                let addons_symbol = session.sync_odoo.get_main_entry().borrow().get_symbol(session.st()).map(|ep_sym_key|
                    session.st().get_symbol(ep_sym_key, (&["odoo", "addons"], &[]), u32::MAX)
                );
                match addons_symbol {
                    Some(addons_symbol) if !addons_symbol.is_empty() => {
                        if let SymbolKey::Namespace(addons) = addons_symbol[0]
                        && let Some(module_symbol) = SymbolTable::create_module_from_path(session, &path_for_tree, addons) {
                        BuildScheduler::queue(session, BuildableSymbolKey::Module(module_symbol));
                        }
                    }
                    _ => {
                        error!("Unable to find addons symbol to create new module");
                    }
                }
            }
        }
    }

    pub fn handle_did_rename(session: &mut SessionInfo, params: RenameFilesParams) {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let old_path = FileMgr::uri2pathname(&f.old_uri);
            let new_path = FileMgr::uri2pathname(&f.new_uri);
            session.log_message(MessageType::INFO, format!("Renaming {} to {}", old_path, new_path));
            //1 - delete old uri
            session.sync_odoo.opened_files.retain(|x| x != &old_path.clone());
            SyncOdoo::unload_path(session, Path::new(&old_path));
            FileMgr::delete_path(session, &old_path);
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&mut session.sync_odoo.symbol_table, &old_path);
            BuildScheduler::process_rebuilds(session, false);
            //2 - create new document
            let new_path_buf = Path::new(&new_path);
            let new_path_updated = new_path_buf.to_tree_path().sanitize();
            Odoo::search_symbols_to_rebuild(session, &new_path_updated);
            BuildScheduler::process_rebuilds(session, false);
            let tree = session.sync_odoo.path_to_main_entry_tree(new_path_buf);
            if let Some(tree) = tree
                &&  new_path_buf.is_file() &&  session.st().get_symbol(session.sync_odoo.get_main_entry().borrow().root.into(), tree.as_slice(), u32::MAX).is_empty() {
                    //file has not been added to main entry. Let's build a new entry point
                    EntryPointMgr::create_new_custom_entry_for_path(session, &new_path_updated, &new_path_buf.sanitize_cow());
                    BuildScheduler::process_rebuilds(session, false);
                }
            BuildScheduler::process_rebuilds(session, false);
        }
    }

    pub fn handle_did_create(session: &mut SessionInfo, params: CreateFilesParams) {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            let path_updated = Path::new(&path).to_tree_path().to_str().unwrap().to_string();
            session.log_message(MessageType::INFO, format!("Creating {}", path.clone()));
            Odoo::search_symbols_to_rebuild(session, &path_updated);
            session.sync_odoo.entry_point_mgr.borrow_mut().clean_entries(&mut session.sync_odoo.symbol_table);
        }
        BuildScheduler::process_rebuilds(session, false);
        //Now let's test if the symbol has been added to main entry tree or not
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            let path_updated = Path::new(&path).to_tree_path().sanitize();
            let tree = session.sync_odoo.path_to_main_entry_tree(Path::new(&path));
            if Path::new(&path).is_file() && (tree.is_none() || (
                session.st().get_symbol(session.sync_odoo.get_main_entry().borrow().root.into(), tree.unwrap().as_slice(), u32::MAX).is_empty()
                && !session.sync_odoo.get_main_entry().borrow().data_symbols.contains_key(&path_updated)
            )) {
                //file has not been added to main entry. Let's build a new entry point
                EntryPointMgr::create_new_custom_entry_for_path(session, &path_updated, &path);
                BuildScheduler::process_rebuilds(session, false);
            }
        }
    }

    pub fn handle_did_delete(session: &mut SessionInfo, params: DeleteFilesParams) {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            session.log_message(MessageType::INFO, format!("Deleting {}", path));
            //1 - delete old uri
            SyncOdoo::unload_path(session, Path::new(&path));
            FileMgr::delete_path(session, &path);
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&mut session.sync_odoo.symbol_table, &Path::new(&path).to_tree_path().sanitize_cow());
        }
        BuildScheduler::process_rebuilds(session, false);
    }

    pub fn handle_did_change(session: &mut SessionInfo, params: DidChangeTextDocumentParams) {
        let (scheme, path) = match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                match params.text_document.uri.to_file_path(){
                    Ok(path) => (schema, path.sanitize()),
                    Err(error) => {
                        warn!("Invalid file URI: {}: {}", params.text_document.uri.to_string(), error);
                        return;
                    }
                }
            },
            Some(scheme) if scheme == "untitled" => (scheme, params.text_document.uri.to_string()),
            Some(scheme) => {
                warn!("Unsupported URI scheme: {}", scheme);
                return;
            },
            None => {
                warn!("No URI scheme found");
                return;
            },
        };
        let path_buf = Path::new(&path);
        session.log_message(MessageType::INFO, format!("File changed: {}", path));
        let file_extension = match scheme.as_str() {
            "file" => path_buf.extension().and_then(|s| s.to_str()).unwrap_or(""),
            "untitled" => "py",
            _ => return,
        };
        let (valid, updated) = Odoo::update_file_cache(session, &path, file_extension, Some(&params.content_changes), params.text_document.version);
        if session.sync_odoo.state_init != InitState::NOT_READY && valid && updated {
            Odoo::update_file_index(session, path_buf, file_extension, false, false);
            if ["js", "ts"].contains(&file_extension)
                && let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                    for change in &params.content_changes {
                        match change.range {
                            Some(range) => {
                                bridge.change_file(
                                    &path,
                                    range.start.line,
                                    range.start.character,
                                    range.end.line,
                                    range.end.character,
                                    &change.text,
                                );
                            }
                            None => {
                                // Full-document replacement — re-open the file with the new content.
                                bridge.open_file(&path, &change.text);
                            }
                        }
                    }
                }
        }
    }

    pub fn handle_did_save(session: &mut SessionInfo, params: DidSaveTextDocumentParams) {
        let path = match params.text_document.uri.to_file_path() {
            Ok(path) => path,
            Err(error) => {
                let msg = format!("Invalid file URI: {}: {}", *params.text_document.uri, error);
                session.log_message(MessageType::ERROR, msg.clone());
                warn!("{}", &msg);
                return;
            }
        };
        if Odoo::check_handle_config_file_update(session, &path) {
            return; //config file update, handled by the config file handler
        }
        session.log_message(MessageType::INFO, format!("File saved: {}", path.sanitize_cow()));
        //No need to update the index on save, as the file change event will do it
        //Odoo::update_file_index(session, path,true, false, false);
    }

    /// Whether `extension` (no leading dot) is a source kind the server processes.
    /// JS/TS are excluded when `disable_javascript` is set.
    fn is_recognized_extension(session: &SessionInfo, extension: &str) -> bool {
        match extension {
            "py" | "xml" | "csv" => true,
            "js" | "ts" => !session.sync_odoo.config.is_javascript_disabled(),
            _ => false,
        }
    }

    // return (valid, updated) booleans
    // if the file has been updated, is valid for an index reload, and contents have been changed
    fn update_file_cache(session: &mut SessionInfo, path: &str, extension: &str, content: Option<&[TextDocumentContentChangeEvent]>, version: i32) -> (bool, bool) {
        if Odoo::is_recognized_extension(session, extension) || Odoo::is_config_workspace_file(session, Path::new(path)){
            session.log_message(MessageType::INFO, format!("File Change Event: {}, version {}", path, version));
            let (file_updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path, content, Some(version), false);
            file_info.borrow_mut().publish_diagnostics(session); //To push potential syntax errors or refresh previous one
            return (!file_info.borrow().opened || version >= 0, file_updated);
        }
        (false, false)
    }

    pub fn update_file_index(session: &mut SessionInfo, path: &Path, extension: &str, _is_open: bool, force_delay: bool) {
        if Odoo::is_recognized_extension(session, extension) || Odoo::is_config_workspace_file(session, path){
            SessionInfo::request_update_file_index(session, path, force_delay);
        }
    }

    pub(crate) fn handle_document_symbols(session: &mut SessionInfo<'_>, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>, ResponseError> {
        session.log_message(MessageType::INFO, format!("Document symbol requested for {}",
            params.text_document.uri.as_str(),
        ));
        let (schema, path) = match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document.uri.to_string();
                if !uri.ends_with(".py") && !uri.ends_with(".pyi") && !uri.ends_with(".xml") && !uri.ends_with(".csv") && !uri.ends_with(".js") && !uri.ends_with(".ts") {
                    return Ok(None);
                }
                match params.text_document.uri.to_file_path(){
                    Ok(path) => (schema, path.sanitize()),
                    Err(error) => {
                        warn!("Invalid file URI: {}: {}", params.text_document.uri.to_string(), error);
                        return Ok(None);
                    }
                }
            },
            Some(schema) if schema == "untitled" => (schema, params.text_document.uri.to_string()),
            Some(scheme) => {
                warn!("Unsupported URI scheme: {}", scheme);
                return Ok(None);
            },
            None => {
                warn!("No URI scheme found");
                return Ok(None);
            }
        };
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
        if let Some(file_info) = file_info {
            if schema != "untitled" && !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                file_info.borrow_mut().prepare_ast(session);
            }
            return Ok(DocumentSymbolFeature::get_symbols(session, &file_info));
        }
        Ok(None)
    }

    pub fn handle_workspace_symbols(session: &mut SessionInfo<'_>, params: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>, ResponseError> {
        session.log_message(MessageType::INFO, format!("Workspace Symbol requested with query {}",
            params.query,
        ));
        WorkspaceSymbolFeature::get_workspace_symbols(session, params.query)
    }

    pub fn handle_workspace_symbols_resolve(session: &mut SessionInfo<'_>, symbol: WorkspaceSymbol) -> Result<WorkspaceSymbol, ResponseError> {
        session.log_message(MessageType::INFO, format!("Workspace Symbol Resolve for symbol {}",
            symbol.name,
        ));
        WorkspaceSymbolFeature::resolve_workspace_symbol(session, &symbol)
    }

    /// Checks if the given path is a configuration file under one of the workspace folders.
    fn is_config_workspace_file(session: &mut SessionInfo, path: &Path) -> bool {
        session.sync_odoo
        .get_file_mgr()
        .borrow()
        .get_processed_workspace_folders()
        .iter()
        .any(|(_, ws_dir)| path.starts_with(ws_dir) && path.ends_with("odools.toml"))
    }

    /// Checks if the given path is a configuration file and handles the update accordingly.
    /// Returns true if the path is a configuration file and was handled, false otherwise.
    fn check_handle_config_file_update(session: &mut SessionInfo, path: &Path) -> bool {
        // Check if the change is affecting a config file
        if Odoo::is_config_workspace_file(session, path) {
            let config_result = config::get_configuration(session)
                .and_then(|(cfg_map, cfg_file)| {
                    let config_name = session.sync_odoo.selected_config.clone().unwrap_or(DEFAULT_PROFILE_NAME.to_string());
                    cfg_map.get(&config_name)
                        .cloned()
                        .ok_or_else(|| format!("Unable to find selected configuration \"{config_name}\""))
                        .map(|config| (config, cfg_file))
                });

            match config_result {
                Ok((new_config, cfg_file)) => {
                    Odoo::handle_config_update(session, new_config, cfg_file);
                }
                Err(err) => {
                    // Invalid config, send a notification to the user and add the error to the logs
                    let msg = format!("Invalid configuration file: {err}.");
                    error!("{msg}");
                    session.show_message(MessageType::ERROR, msg.clone());
                    // The popup is transient; also record it as a persistent diagnostic so
                    // it stays visible in the status bar/tooltip until the config is fixed.
                    session.send_config_diagnostic(ConfigDiagnosticAction::EXTEND, &[
                        ConfigDiagnosticMessage {
                            level: ConfigDiagnosticMessageLevel::ERROR,
                            message: msg,
                        }
                    ]);
                }
            }
            true
        }
        else {
            false
        }
    }

    pub fn handle_config_update(session: &mut SessionInfo, new_config: ConfigEntry, cfg_file: ConfigView) {
        if config::needs_restart(&session.sync_odoo.config, &new_config) {
            // Changes require a restart, ask the client to restart the server
            session.send_notification("$Odoo/restartNeeded", ());
            return;
        }
        // Changes can be applied without restart
        let languages_changed = session.sync_odoo.config.additional_languages() != new_config.additional_languages();
        session.sync_odoo.config_file = Some(cfg_file);
        // Recalculate diagnostic filters
        session.sync_odoo.config = new_config;
        session.sync_odoo.get_file_mgr().borrow_mut().update_all_file_diagnostic_filters(session);
        session.update_delay_thread_delay_duration(session.sync_odoo.config.auto_refresh_delay());
        if languages_changed {
            SyncOdoo::revalidate_language_dependents(session);
            BuildScheduler::process_rebuilds(session, false);
        }
    }

    pub fn handle_tsserver_new_diagnostics(session: &mut SessionInfo<'_>, msg: TsServerDiagnostics) {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let file_path = Path::new(&msg.file).sanitize();
        if let Some(file_info) = file_mgr.borrow().get_file_info(&file_path) {
            //as we receive line and character from tsserver, transform into offset to store in (offset, 0) structure
            //TODO maybe find a way to handle it properly as XML is doing the same
            let diagnostics: Vec<Diagnostic> = msg.diagnostics.iter().map(|d| {
                let mut new_d = d.clone();
                new_d.range = Range{
                    start: Position {
                        line: file_info.borrow().position_to_offset(d.range.start.line, d.range.start.character, session.sync_odoo.encoding) as u32,
                        character: 0
                    },
                    end: Position {
                        line: file_info.borrow().position_to_offset(d.range.end.line, d.range.end.character, session.sync_odoo.encoding) as u32,
                        character: 0
                    },
                };
                new_d
            }).collect();
            file_info.borrow_mut().replace_diagnostics(msg.diagnostic_level, diagnostics); //TsServer will alwayse use ARCH/ARCH_EVAL and VALIDATION level to store diagnostics, while SYNTAX is reserved to OXC
            file_info.borrow_mut().publish_diagnostics(session);
        } else {
            warn!("Received diagnostics for unknown file: {}", msg.file);
        }
    }
}
