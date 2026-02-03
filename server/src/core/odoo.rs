use crate::constants::OYarn;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::entry_point::EntryPointType;
use crate::core::file_mgr::AstType;
use crate::core::module_load_order::sort_by_load_order;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::symbol_table::SymbolTable;
use crate::core::symbols::symbol_keys::{ContainsKey, FunctionKey, ModuleKey, SymbolKey, Weak};
use crate::core::xml_data::OdooData;
use crate::core::xml_validation::XmlValidator;
use crate::fifo_ptr_weak_hash_set::FifoWeakHashSet;
// use crate::features::document_symbols::DocumentSymbolFeature;
// use crate::features::references::ReferenceFeature;
// use crate::features::workspace_symbols::WorkspaceSymbolFeature;
use crate::threads::SessionInfo;
// use crate::features::completion::CompletionFeature;
// use crate::features::definition::DefinitionFeature;
// use crate::features::hover::HoverFeature;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::{Rc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use lsp_server::{ErrorCode, RequestId, ResponseError};
use lsp_types::notification::{Notification, Progress};
use lsp_types::request::WorkDoneProgressCreate;
use lsp_types::*;
use request::{RegisterCapability, Request, WorkspaceConfiguration};
use ruff_source_file::PositionEncoding;
use serde_json::Value;
use tracing::{error, warn, info, trace};

use std::collections::HashSet;
use std::process::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use regex::Regex;
use crate::{constants::*, oyarn, Sy};
use super::config::{self, default_profile_name, get_configuration, ConfigEntry, ConfigFile};
use super::entry_point::{EntryPoint, EntryPointMgr};
use super::file_mgr::FileMgr;
use super::import_resolver::ImportCache;
use crate::core::model::Model;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::core::python_arch_eval::PythonArchEval;
use crate::core::python_validator::PythonValidator;
use crate::utils::{PathSanitizer, ToFilePath as _};
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
pub struct SyncOdoo {
    pub version_major: u32,
    pub version_minor: u32,
    pub version_micro: u32,
    pub full_version: String,
    pub python_version: Vec<u32>,
    pub config: ConfigEntry,
    pub selected_config_profile: Option<String>,
    pub config_file: Option<ConfigFile>,
    pub config_path: Option<String>,
    pub entry_point_mgr: Rc<RefCell<EntryPointMgr>>, //An Rc to be able to clone it and free session easily
    pub has_main_entry:bool,
    pub has_odoo_main_entry: bool,
    pub has_valid_python: bool,
    pub main_entry_tree: Vec<OYarn>,
    pub stubs_dirs: Vec<String>,
    pub stdlib_dir: String,
    pub progress_token: i32,
    file_mgr: Rc<RefCell<FileMgr>>,
    pub modules: HashMap<OYarn, Weak<ModuleKey>>,
    pub models: HashMap<OYarn, Rc<RefCell<Model>>>,
    pub interrupt_rebuild: Arc<AtomicBool>,
    pub terminate_rebuild: Arc<AtomicBool>,
    pub current_request_id: Option<RequestId>,
    pub running_request_ids: Arc<Mutex<Vec<RequestId>>>, //Arc to Server mutex for cancellation support
    pub watched_file_updates: u32,
    rebuild_arch: FifoWeakHashSet<SymbolKey>,
    rebuild_arch_eval: FifoWeakHashSet<SymbolKey>,
    rebuild_validation: FifoWeakHashSet<SymbolKey>,
    pub state_init: InitState,
    pub must_reload_paths: Vec<(Weak<SymbolKey>, String)>, // formerly Weak refs
    pub load_odoo_addons: bool, //indicate if we want to load odoo addons or not
    pub need_rebuild: bool, //if true, the next process_rebuilds will drop everything and rebuild everything
    pub import_cache: Option<ImportCache>,
    pub capabilities: lsp_types::ClientCapabilities,
    pub encoding: PositionEncoding,
    pub opened_files: Vec<String>,
    pub symbol_table: SymbolTable,
    pub evaluation_search: Option<SymbolKey>, //If set, any evaluation will be check against this value. If evaluation matches, location is kept in evaluation_locations
    pub evaluation_locations: Vec<Location>,

    pub test_mode: bool,
}

unsafe impl Send for SyncOdoo {}

impl SyncOdoo {

    pub fn new() -> Self {
        let sync_odoo = Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            python_version: vec![0, 0, 0],
            config: ConfigEntry::new(),
            selected_config_profile: None,
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
            modules: HashMap::new(),
            models: HashMap::new(),
            interrupt_rebuild: Arc::new(AtomicBool::new(false)),
            terminate_rebuild: Arc::new(AtomicBool::new(false)),
            current_request_id: None,
            running_request_ids: Arc::new(Mutex::new(vec![])),
            watched_file_updates: 0,
            rebuild_arch: FifoWeakHashSet::new(),
            rebuild_arch_eval: FifoWeakHashSet::new(),
            rebuild_validation: FifoWeakHashSet::new(),
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

            test_mode: false,
        };
        sync_odoo
    }

    pub fn reset(session: &mut SessionInfo, config: ConfigEntry) {
        session.log_message(MessageType::INFO, S!("Resetting Database..."));
        info!("Resetting database...");
        session.sync_odoo.version_major = 0;
        session.sync_odoo.version_minor = 0;
        session.sync_odoo.version_micro = 0;
        session.sync_odoo.full_version = "0.0.0".to_string();
        session.sync_odoo.config = ConfigEntry::new();
        FileMgr::clear(session);//only reset files, as workspace folders didn't change
        session.sync_odoo.stubs_dirs = SyncOdoo::default_stubs();
        session.sync_odoo.stdlib_dir = SyncOdoo::default_stdlib();
        session.sync_odoo.modules = HashMap::new();
        session.sync_odoo.models = HashMap::new();
        session.sync_odoo.rebuild_arch = FifoWeakHashSet::new();
        session.sync_odoo.rebuild_arch_eval = FifoWeakHashSet::new();
        session.sync_odoo.rebuild_validation = FifoWeakHashSet::new();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.sync_odoo.load_odoo_addons = true;
        session.sync_odoo.need_rebuild = false;
        session.sync_odoo.watched_file_updates = 0;
        //drop all entries, except entries of opened files
        session.sync_odoo.entry_point_mgr.borrow_mut().reset_entry_points(false);
        session.sync_odoo.symbol_table = SymbolTable::new();
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
        session.sync_odoo.symbol_table.pre_allocate();
        info!("Initializing odoo");
        info!("Full Config: {:?}", config);
        let start_time = Instant::now();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.send_notification("$Odoo/loadingStatusUpdate", "start");
        session.sync_odoo.config = config;
        if session.sync_odoo.config.no_typeshed_stubs {
            session.sync_odoo.stubs_dirs.clear();
        }
        for stub in session.sync_odoo.config.additional_stubs.iter() {
            session.sync_odoo.stubs_dirs.push(PathBuf::from(stub.clone()).sanitize());
        }
        if !session.sync_odoo.config.stdlib.is_empty() {
            session.sync_odoo.stdlib_dir = PathBuf::from(session.sync_odoo.config.stdlib.clone()).sanitize();
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
            match Command::new(session.sync_odoo.config.python_path.clone()).args(&["-c", "import sys; import json; print(json.dumps(sys.path))"]).output() {
                Err(err) => {
                    warn!("Wrong python command: {}, error: {}", session.sync_odoo.config.python_path.clone(), err);
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
                            let pathbuf = PathBuf::from(path);
                            if pathbuf.is_dir() {
                                let final_path = pathbuf.sanitize();
                                session.log_message(MessageType::INFO, format!("Adding sys.path: {}", final_path));
                                EntryPointMgr::add_entry_to_public(session, final_path.clone());
                            }
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        warn!("Error reading sys.path: {}", stderr);
                    }
                }
            }
            match Command::new(session.sync_odoo.config.python_path.clone()).args(&["-c", "import sys; import json; print(json.dumps(sys.version_info))"]).output() {
                Err(err) => {
                    warn!("Wrong python command: {}, error: {}", session.sync_odoo.config.python_path.clone(), err);
                    session.send_notification("$Odoo/invalid_python_path", ());
                },
                Ok(output) => {
                    session.sync_odoo.has_valid_python = true;
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        session.log_message(MessageType::INFO, format!("Detected sys.version_info: {}", stdout));
                        let version_infos: Value = serde_json::from_str(&stdout).expect("Unable to get python version info with json of sys.version_info output");
                        session.sync_odoo.python_version = version_infos.as_array()
                            .expect("Expected JSON array")
                            .iter()
                            .filter_map(|v| v.as_u64())
                            .map(|v| v as u32)
                            .take(3)
                            .collect();
                        info!("Detected python version: {}.{}.{}", session.sync_odoo.python_version[0], session.sync_odoo.python_version[1], session.sync_odoo.python_version[2]);
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        warn!("Error reading sys.version_info: {}", stderr);
                    }
                }
            }
        }
        if SyncOdoo::load_builtins(session) {
            session.sync_odoo.state_init = InitState::PYTHON_READY;
            SyncOdoo::build_database(session);
        }
        session.send_notification("$Odoo/loadingStatusUpdate", "stop");
        session.log_message(MessageType::INFO, format!("End of initialization. Time taken: {} ms", start_time.elapsed().as_millis()));
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
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let path = PathBuf::from(&session.sync_odoo.stdlib_dir);
        let builtins_path = path.join("builtins.pyi");
        if !builtins_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find builtins.pyi. Are you sure that typeshed has been downloaded. If you are building from source, make sure to initialize submodules with 'git submodule init' and 'git submodule update'."));
            error!("Unable to find builtins at: {}", builtins_path.sanitize());
            return false;
        };
        let tree_builtins = path.to_tree();
        let entry_stdlib = session.sync_odoo.find_stdlib_entry_point();
        let disk_dir_builtins = st!().get_symbol(entry_stdlib.borrow().root.into(), &tree_builtins, u32::MAX);
        if disk_dir_builtins.is_empty() {
            panic!("Unable to find builtins disk dir symbol");
        }
        let _builtins_rc_symbol = SymbolTable::create_from_path(session, &builtins_path, disk_dir_builtins[0], false);
        session.sync_odoo.add_to_rebuild_arch(_builtins_rc_symbol.unwrap());
        SyncOdoo::process_rebuilds(session, false)
    }

    pub fn build_database(session: &mut SessionInfo) {
        session.log_message(MessageType::INFO, String::from("Building Database"));
        // Self::log_capacities(session);
        let result = SyncOdoo::build_base(session);
        if result {
            SyncOdoo::build_modules(session);
        }

        // Self::log_counts(session);
        Self::print_memory();
        // Self::log_capacities(session);
    }

    // fn log_counts(session: &mut SessionInfo) {
    //     let st = &session.sync_odoo.symbol_table;
    //     info!("Symbol table roots count: {}", st.roots.len());
    //     info!("Symbol table disk_dirs count: {}", st.disk_dirs.len());
    //     info!("Symbol table namespaces count: {}", st.namespaces.len());
    //     info!("Symbol table python packages count: {}", st.python_packages.len());
    //     info!("Symbol table modules count: {}", st.modules.len());
    //     info!("Symbol table files count: {}", st.files.len());
    //     info!("Symbol table compiled count: {}", st.compiled.len());
    //     info!("Symbol table classes count: {}", st.classes.len());
    //     info!("Symbol table functions count: {}", st.functions.len());
    //     info!("Symbol table variables count: {}", st.variables.len());
    //     info!("Symbol table xml_files count: {}", st.xml_files.len());
    //     info!("Symbol table csv_files count: {}", st.csv_files.len());
    // }

    // fn log_capacities(session: &mut SessionInfo) {
    //     let st = &session.sync_odoo.symbol_table;
    //     info!("Symbol table capacities - roots: {}, disk_dirs: {}, namespaces: {}, python packages: {}, modules: {}, files: {}, compiled: {}, classes: {}, functions: {}, variables: {}, xml_files: {}, csv_files: {}",
    //         st.roots.capacity(),
    //         st.disk_dirs.capacity(),
    //         st.namespaces.capacity(),
    //         st.python_packages.capacity(),
    //         st.modules.capacity(),
    //         st.files.capacity(),
    //         st.compiled.capacity(),
    //         st.classes.capacity(),
    //         st.functions.capacity(),
    //         st.variables.capacity(),
    //         st.xml_files.capacity(),
    //         st.csv_files.capacity()
    //     );
    // }

    fn print_memory() {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") || line.starts_with("VmPeak:") || line.starts_with("VmHWM:") {
                    eprintln!("Symbol table {}", line);
                }
            }
        }
    }

    pub fn read_version(session: &mut SessionInfo, release_path: PathBuf) -> (u32, u32, u32) {
        let mut _version_major: u32 = 14;
        let mut _version_minor: u32 = 0;
        let mut _version_micro: u32 = 0;
        // open release.py and get version
        let release_file = fs::read_to_string(release_path.sanitize());
        let release_file = match release_file {
            Ok(release_file) => release_file,
            Err(_) => {
                session.log_message(MessageType::INFO, String::from("Unable to read release.py - Aborting"));
                return (0, 0, 0);
            }
        };
        for line in release_file.lines() {
            if line.starts_with("version_info = (") {
                let result = VERSION_REGEX.captures(line);
                match result {
                    Some(result) => {
                        let version_info = result.get(1).unwrap().as_str();
                        let version_info = version_info.split(", ").collect::<Vec<&str>>();
                        let version_major = version_info[0].replace("saas~", "").replace("'", "").replace(r#"""#, "");
                        _version_major = version_major.parse().unwrap();
                        _version_minor = version_info[1].parse().unwrap();
                        _version_micro = version_info[2].parse().unwrap();
                        break;
                    },
                    None => {
                        session.log_message(MessageType::ERROR, String::from("Unable to detect the Odoo version. Running the tool for the version 14"));
                        break;
                    }
                }
            }
        }
        (_version_major, _version_minor, _version_micro)
    }

    fn build_base(session: &mut SessionInfo) -> bool {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let odoo_path = session.sync_odoo.config.odoo_path.clone();
        let Some(odoo_path) = odoo_path.filter(|odoo_path| PathBuf::from(odoo_path.clone()).exists()) else {
            info!("Odoo path not provided or is not a valid path. Continuing in single file mode");
            return false;
        };
        session.sync_odoo.has_main_entry = true;
        let odoo_sym = EntryPointMgr::set_main_entry(session, odoo_path.clone());
        let odoo_entry = session.sync_odoo.entry_point_mgr.borrow().main_entry_point.as_ref().unwrap().clone();
        session.sync_odoo.main_entry_tree = odoo_entry.borrow().tree.clone();
        let release_path = PathBuf::from(odoo_path.clone()).join("odoo/release.py");
        let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
        if !release_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find release.py - Aborting and switching to non-odoo mode"));
            return false;
        }
        let (_version_major, _version_minor, _version_micro) = SyncOdoo::read_version(session, release_path);
        if _version_major == 0 {
            return false;
        }
        let _full_version = format!("{}.{}.{}", _version_major, _version_minor, _version_micro);
        session.log_message(MessageType::INFO, format!("Odoo version: {}", _full_version));
        if _version_major < 14 {
            session.log_message(MessageType::ERROR, String::from("Odoo version is less than 14. The tool only supports version 14 and above. Aborting and switching to non-odoo mode"));
            return false;
        }
        session.sync_odoo.version_major = _version_major;
        session.sync_odoo.version_minor = _version_minor;
        session.sync_odoo.version_micro = _version_micro;
        session.sync_odoo.full_version = _full_version;
        //build base
        let config_odoo_path = PathBuf::from(odoo_path.clone());
        let Some(odoo_sym) = odoo_sym else {
            panic!("Odoo root symbol not found")
        };
        st!().set_is_external(odoo_sym, false);
        let odoo_odoo = SymbolTable::create_from_path(session, &config_odoo_path.join("odoo"), odoo_sym, false);
        let Some(odoo_odoo) = odoo_odoo else {
            panic!("Not able to find odoo with given path. Aborting...");
        };
        match odoo_odoo {
            SymbolKey::PythonPackage(p) => {
                st!()[p].self_import = true;
                session.sync_odoo.add_to_rebuild_arch(odoo_odoo);
            },
            SymbolKey::Namespace(_) => {
                //starting from > 18.0, odoo is now a namespace. Start import project from odoo/__main__.py
                let main_file = SymbolTable::create_from_path(session, &PathBuf::from(config_odoo_path.clone()).join("odoo").join("__main__.py"),  odoo_odoo, false);
                let Some(main_file) = main_file else {
                    panic!("Not able to find odoo/__main__.py. Aborting...");
                };
                let f = main_file.unwrap_file_key();
                st!()[f].self_import = true;
                session.sync_odoo.add_to_rebuild_arch(main_file);
            },
            _ => panic!("Root symbol is not a package or namespace (> 18.0)")
        }
        session.sync_odoo.has_odoo_main_entry = true; // set it now has we need it to parse base addons
        if !SyncOdoo::process_rebuilds(session, false) {
            return false;
        }
        //search common odoo addons path
        let mut addon_symbol = session.sync_odoo.get_symbol(&odoo_path, &tree(vec!["odoo", "addons"], vec![]), u32::MAX);
        if addon_symbol.is_empty() {
            let odoo = session.sync_odoo.get_symbol(&odoo_path, &tree(vec!["odoo"], vec![]), u32::MAX);
            if odoo.is_empty() {
                session.log_message(MessageType::WARNING, "Odoo not found. Switching to non-odoo mode...".to_string());
                session.sync_odoo.has_odoo_main_entry = false;
                return false;
            }
            //if we are > 18.1, odoo.addons is not imported automatically anymore. Let's try to import it manually
            let addons_folder = SymbolTable::create_from_path(session, &PathBuf::from(config_odoo_path).join("odoo").join("addons"), odoo_odoo, false);
            if let Some(addons) = addons_folder {
                addon_symbol = vec![addons];
            } else {
                session.log_message(MessageType::WARNING, "Not able to find odoo/addons. Please check your configuration. Switching to non-odoo mode...".to_string());
                session.sync_odoo.has_odoo_main_entry = false;
                return false;
            }
        }
        // @arena: not in the original code!
        let addon_symbol = addon_symbol[0].unwrap_namespace_key();
        if odoo_addon_path.exists() {
            if session.sync_odoo.load_odoo_addons {
                let path = odoo_addon_path.sanitize();
                st!()[addon_symbol].add_path(path.clone());
                EntryPointMgr::add_entry_to_addons(session, path,
                    Some(odoo_entry.clone()),
                    Some(vec![Sy!("odoo"),
                        Sy!("addons")]));
            }
        } else {
            session.log_message(MessageType::WARNING, format!("Unable to find odoo addons path at {}. You can ignore this message if you use a nightly build or if your community addons are in another addon paths.", odoo_addon_path.sanitize()));
        }
        for addon in session.sync_odoo.config.addons_paths.clone() {
            let addon_path = PathBuf::from(&addon);
            if addon_path.exists() {
                st!()[addon_symbol].add_path(addon_path.sanitize());
                EntryPointMgr::add_entry_to_addons(session, addon,
                    Some(odoo_entry.clone()),
                    Some(vec![Sy!("odoo"),
                        Sy!("addons")]));
            }
        }
        return true;
    }

    fn build_modules(session: &mut SessionInfo) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let Some(addons_symbol) = session.sync_odoo.get_symbol(
            session.sync_odoo.config.odoo_path.as_ref().unwrap(), &tree(vec!["odoo", "addons"], vec![]), u32::MAX
        ).first().copied() else {
            let message = S!("OdooLS: Unable to find 'odoo/addons'. Check the addons_paths in your config or your file structure. Skipping addons loading...");
            warn!("{}", message);
            session.show_message(MessageType::WARNING, message);
            return;
        };
        // @arena: not in the original code. Consider wrapping get_symbol in a get_addon_namespace function
        let addons_path = st!()[addons_symbol.unwrap_namespace_key()].paths();
        let mut modules = vec![];
        for addon_path in addons_path.iter() {
            info!("searching modules in {}", addon_path);
            if PathBuf::from(addon_path).exists() {
                //browse all dir in path
                for item in PathBuf::from(addon_path).read_dir().expect("Unable to browse and odoo addon directory") {
                    if let Ok(item) = item {
                        if item.file_type().unwrap().is_dir() && !session.sync_odoo.modules.contains_key(&oyarn!("{}", item.file_name().to_str().unwrap())) {
                            if let Some(module_symbol) = SymbolTable::create_module_from_path(session, &item.path(), addons_symbol) {
                                modules.push(module_symbol);
                            }
                        }
                    }
                }
            }
        }
        let sorted_modules = SyncOdoo::sort_modules(&st!(), modules);
        for module_symbol in sorted_modules {
            session.sync_odoo.add_to_rebuild_arch(module_symbol.into());
        }
        if !SyncOdoo::process_rebuilds(session, false) {
            return;
        }
        //println!("{}", self.symbols.as_ref().unwrap().borrow_mut().debug_print_graph());
        //fs::write("out_architecture.json", self.get_symbol(&tree(vec!["odoo", "addons", "module_1"], vec![])).as_ref().unwrap().borrow().debug_to_json().to_string()).expect("Unable to write file");
        let modules_count = session.sync_odoo.modules.len();
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
    pub fn get_symbol(&self, from_path: &str, tree: &Tree, position: u32) -> Vec<SymbolKey> {
        //find which entrypoint to use
        for entry in self.entry_point_mgr.borrow().iter_all() {
            let entry_point = entry.borrow();
            if entry_point.is_public() || PathBuf::from(from_path).starts_with(&entry_point.path) {
                let tree: Tree = (entry_point.addon_to_odoo_tree.as_ref().unwrap_or(&entry_point.tree).iter().chain(&tree.0).map(|x| x.clone()).collect(), tree.1.clone());
                let symbols = self.symbol_table.get_symbol(entry_point.root.into(), &tree, position);
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

    fn pop_item(&mut self, step: BuildSteps) -> Option<SymbolKey> {
        //Part 1: Find the symbol with a unmutable set
        let set =  match step {
            BuildSteps::ARCH_EVAL => &self.rebuild_arch_eval,
            BuildSteps::VALIDATION => &self.rebuild_validation,
            _ => &self.rebuild_arch
        };
        let mut selected_sym: Option<SymbolKey> = None;
        let mut selected_count: u32 = 999999999;
        let mut current_count: u32;
        for sym in set.iter_valid(|&k| self.symbol_table.contains_key(k)) {
            current_count = 0;
            let file = self.symbol_table.get_file(sym).unwrap();
            let all_dep = self.symbol_table.get_all_dependencies(file, step);
            if let Some(all_dep) = all_dep {
                for (index, dep_set) in all_dep.iter().enumerate() {
                    if let Some(dep_set) = dep_set {
                         let index_set =  match index {
                                x if x == BuildSteps::ARCH as usize => &self.rebuild_arch,
                                x if x == BuildSteps::ARCH_EVAL as usize => &self.rebuild_arch_eval,
                                x if x == BuildSteps::VALIDATION as usize => &self.rebuild_validation,
                                _ => continue,
                            };
                        current_count +=
                            dep_set.iter_valid(|&k| self.symbol_table.contains_key(k))
                                .filter(|dep| index_set.contains(dep))
                                .count() as u32;
                    }
                }
            }
            if current_count < selected_count {
                selected_sym = Some(sym);
                selected_count = current_count;
                if current_count == 0 {
                    break;
                }
            }
        }
        let set =  match step {
            BuildSteps::ARCH_EVAL => &mut self.rebuild_arch_eval,
            BuildSteps::VALIDATION => &mut self.rebuild_validation,
            _ => &mut self.rebuild_arch
        };
        if selected_sym.is_none() {
            set.clear(); //remove any potential dead weak ref
            return None;
        }
        let selected_sym_unwrapped = selected_sym.unwrap();
        if !set.remove(&selected_sym_unwrapped) {
            panic!("Unable to remove selected symbol from rebuild set")
        }
        return Some(selected_sym_unwrapped);
    }

    fn add_from_self_reload(session: &mut SessionInfo) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        for (weak_sym, path) in session.sync_odoo.must_reload_paths.clone().iter() {
            let Some(parent) = weak_sym.upgrade(&st!()) else {
                continue;
            };
            let in_addons = SymbolTable::get_main_entry_tree(session, parent) == tree(vec!["odoo", "addons"], vec![]);
            let new_symbol = SymbolTable::create_from_path(session, &PathBuf::from(path), parent, in_addons);
            let Some(new_symbol) = new_symbol else {
                continue;
            };
            session.sync_odoo.must_reload_paths.retain(|(_, p)| p != path);
            st!().set_is_external(new_symbol, false);
            match new_symbol {
                SymbolKey::PythonPackage(p) => {
                    st!()[p].self_import = true;
                }
                SymbolKey::File(f) => {
                    st!()[f].self_import = true;
                },
                SymbolKey::Module(_) => {}
                SymbolKey::Namespace(_) => continue, // A module became a namespace, due to __init__ deletion/renaming
                _ => {panic!("Unexpected symbol type: {:?}", new_symbol);}
            }
            if let SymbolKey::Module(m) = new_symbol {
                let name = st!()[m].name.clone();
                session.sync_odoo.modules.insert(name, m.into());
            }
            session.sync_odoo.add_to_rebuild_arch(new_symbol);
        }
        session.sync_odoo.must_reload_paths.retain(|x| x.0.upgrade(&st!()).is_some());
    }

    fn start_reporting(session: &mut SessionInfo) {
        session.sync_odoo.progress_token += 1;
        let _ = session.send_request::<WorkDoneProgressCreateParams, ()>(WorkDoneProgressCreate::METHOD, WorkDoneProgressCreateParams {
            token: ProgressToken::Number(session.sync_odoo.progress_token)
        });
        session.send_notification(Progress::METHOD, ProgressParams {
            token: ProgressToken::Number(session.sync_odoo.progress_token),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(WorkDoneProgressBegin {
                title: "Odoo: Indexing".to_string(),
                cancellable: Some(false),
                message: None,
                percentage: None,
            }))
        });
    }

    pub fn process_rebuilds(session: &mut SessionInfo, no_validation: bool) -> bool {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
        if session.sync_odoo.watched_file_updates > MAX_WATCHED_FILES_UPDATES_BEFORE_RESTART {
            return false;
        }
        SyncOdoo::add_from_self_reload(session);
        session.sync_odoo.import_cache = Some(ImportCache{ modules: HashMap::new(), main_modules: HashMap::new() });
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::new();

        //workdone progress
        let mut last_update_status = Instant::now() - Duration::from_secs(10);
        let mut is_reporting_progress = false;
        if !session.sync_odoo.rebuild_arch.is_empty() || !session.sync_odoo.rebuild_arch_eval.is_empty() || !session.sync_odoo.rebuild_validation.is_empty() {
            is_reporting_progress = true;
            SyncOdoo::start_reporting(session);
        }
        trace!("Starting rebuild: {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_validation.len());
        while !session.sync_odoo.need_rebuild && (!session.sync_odoo.rebuild_arch.is_empty() || !session.sync_odoo.rebuild_arch_eval.is_empty() || !session.sync_odoo.rebuild_validation.is_empty()) {
            if DEBUG_THREADS {
                trace!("remains: {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_validation.len());
            }
            let queue_size = session.sync_odoo.rebuild_arch.len() * 3 + session.sync_odoo.rebuild_arch_eval.len() * 2 + session.sync_odoo.rebuild_validation.len();
            if is_reporting_progress && (Instant::now() - last_update_status) > Duration::from_millis(200) {
                last_update_status = Instant::now();
                session.send_notification(Progress::METHOD, ProgressParams {
                    token: ProgressToken::Number(session.sync_odoo.progress_token),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(WorkDoneProgressReport {
                        cancellable: Some(false),
                        message: Some(format!("{} items remaining", queue_size)),
                        percentage: None,
                    }))
                });
            }
            if session.sync_odoo.terminate_rebuild.load(Ordering::SeqCst){
                info!("Terminating rebuilds due to server shutdown");
                return false;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::ARCH);
            if let Some(sym_key) = sym {
                if DEBUG_STEPS {
                    trace!("PROCESSING FROM ARCH - {}", st!().debug_path(sym_key));
                }
                let (tree, entry) = st!().get_tree_and_entry(sym_key);
                if already_arch_rebuilt.contains(&tree) {
                    info!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                let mut builder = PythonArchBuilder::new(entry.unwrap(), sym_key);
                builder.load_arch(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::ARCH_EVAL);
            if let Some(sym_key) = sym {
                if DEBUG_STEPS {
                    trace!("PROCESSING FROM ARCH_EVAL - {}", st!().debug_path(sym_key));
                }
                let (tree, entry) = st!().get_tree_and_entry(sym_key);
                if already_arch_eval_rebuilt.contains(&tree) {
                    info!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                let mut builder = PythonArchEval::new(entry.unwrap(), sym_key);
                builder.eval_arch(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::VALIDATION);
            if let Some(sym_key) = sym {
                if DEBUG_STEPS {
                    trace!("PROCESSING FROM VALIDATION - {}", st!().debug_path(sym_key));
                }
                let (_, entry) = st!().get_tree_and_entry(sym_key);
                if session.sync_odoo.state_init == InitState::ODOO_READY {
                    let mut no_validation = no_validation;
                    if session.sync_odoo.interrupt_rebuild.load(Ordering::SeqCst) {
                        session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
                        session.log_message(MessageType::INFO, S!("Rebuild interrupted"));
                        no_validation = true;
                    }
                    if no_validation {
                        session.request_delayed_rebuild();
                        session.sync_odoo.add_to_validations(sym_key);
                        if is_reporting_progress {
                            session.send_notification(Progress::METHOD, ProgressParams {
                                token: ProgressToken::Number(session.sync_odoo.progress_token),
                                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                                    message: None,
                                }))
                            });
                        }
                        return true;
                    }
                }
                match sym_key {
                    SymbolKey::XmlFile(xml) => {
                        let mut validator = XmlValidator::new(entry.as_ref().unwrap(), xml);
                        validator.validate(session);
                    },
                    _ => {
                        let mut validator = PythonValidator::new(entry.unwrap(), sym_key);
                        validator.validate(session);
                    }
                }
                continue;
            }
        }
        if session.sync_odoo.need_rebuild {
            session.log_message(MessageType::INFO, S!("Rebuild required. Resetting database on breaktime..."));
            info!("Odoo version change detected. OdooLS is restarting");
            session.send_notification("$Odoo/restartNeeded", ());
        }
        session.sync_odoo.import_cache = None;
        session.sync_odoo.watched_file_updates = 0;
        if is_reporting_progress {
            session.send_notification(Progress::METHOD, ProgressParams {
                token: ProgressToken::Number(session.sync_odoo.progress_token),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                    message: None,
                }))
            });
        }
        trace!("Leaving rebuild with remaining tasks: {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_validation.len());
        true
    }

    pub fn add_to_rebuild_arch(&mut self, symbol: SymbolKey) {
        if DEBUG_THREADS {
            trace!("ADDED TO ARCH - {}", self.symbol_table.debug_path(symbol));
        }
        if self.symbol_table.build_status(symbol, BuildSteps::ARCH) != BuildStatus::IN_PROGRESS {
            self.symbol_table.set_build_status(symbol, BuildSteps::ARCH, BuildStatus::PENDING);
            self.symbol_table.set_build_status(symbol, BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            self.symbol_table.set_build_status(symbol, BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_arch.insert(symbol);
        }
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: SymbolKey) {
        if DEBUG_THREADS {
            trace!("ADDED TO EVAL - {}", self.symbol_table.debug_path(symbol));
        }
        if self.symbol_table.build_status(symbol, BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS {
            self.symbol_table.set_build_status(symbol, BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            self.symbol_table.set_build_status(symbol, BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_arch_eval.insert(symbol);
        }
    }

    pub fn add_to_validations(&mut self, symbol: SymbolKey) {
        if DEBUG_THREADS {
            trace!("ADDED TO VALIDATION - {}", self.symbol_table.debug_path(symbol));
        }
        if self.symbol_table.build_status(symbol, BuildSteps::VALIDATION) != BuildStatus::IN_PROGRESS {
            self.symbol_table.set_build_status(symbol, BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_validation.insert(symbol);
        }
    }

    /* Ask for an immediate rebuild of the given symbol if possible.
    return true if a rebuild has been done
     */
    pub fn build_now(session: &mut SessionInfo, symbol: SymbolKey, step: BuildSteps) -> bool {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        match symbol {
            SymbolKey::Root(_) | SymbolKey::Namespace(_) | SymbolKey::DiskDir(_) | SymbolKey::Compiled(_) | SymbolKey::Class(_) | SymbolKey::Variable(_)  => return false,
            _ => {}
        }
        if DEBUG_REBUILD_NOW {
            if st!().build_status(symbol, step) == BuildStatus::INVALID {
                panic!("Trying to build an invalid symbol: {}", st!().debug_path(symbol));
            }
            if st!().build_status(symbol, step) == BuildStatus::IN_PROGRESS && !session.sync_odoo.is_in_rebuild(symbol, step) {
                error!("Trying to build a symbol that is NOT in the queue: {}", st!().debug_path(symbol));
            }
        }
        if st!().build_status(symbol, step) == BuildStatus::PENDING && st!().previous_step_done(symbol, step) {
            SyncOdoo::build_now_dependencies(session, symbol, step);
            let entry_point = st!().get_entry(symbol).unwrap();
            session.sync_odoo.remove_from_rebuild(symbol, step);
            if step == BuildSteps::ARCH {
                let mut builder = PythonArchBuilder::new(entry_point, symbol);
                builder.load_arch(session);
                return true;
            } else if step == BuildSteps::ARCH_EVAL {
                if DEBUG_REBUILD_NOW {
                    if st!().build_status(symbol, BuildSteps::ARCH) != BuildStatus::DONE {
                        panic!("An evaluation has been requested on a non-arched symbol: {}", st!().debug_path(symbol));
                    }
                }
                let mut builder = PythonArchEval::new(entry_point, symbol);
                builder.eval_arch(session);
                return true;
            } else if step == BuildSteps::VALIDATION {
                if DEBUG_REBUILD_NOW {
                    if st!().build_status(symbol, BuildSteps::ARCH) != BuildStatus::DONE || st!().build_status(symbol, BuildSteps::ARCH_EVAL) != BuildStatus::DONE {
                        panic!("An evaluation has been requested on a non-arched symbol: {}", st!().debug_path(symbol));
                    }
                }
                let mut validator = PythonValidator::new(entry_point, symbol.clone());
                validator.validate(session);
                return true;
            }
        }
        false
    }

    /// Ensure that a function symbol's evaluations are as fully populated
    pub fn ensure_func_evaluations(session: &mut SessionInfo, function_key: FunctionKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let Some(func_file) = st!().get_file(function_key.into()) else {
            return;
        };
        if st!()[function_key].evaluations.is_empty() && !st!().is_external(func_file) {
            // Run Arch eval on file, if possible, then run everything on the fn
            // until arch_eval
            SyncOdoo::build_now(session, func_file, BuildSteps::ARCH_EVAL);
            SyncOdoo::build_now(session, function_key.into(), BuildSteps::ARCH);
            SyncOdoo::build_now(session, function_key.into(), BuildSteps::ARCH_EVAL);
        }
    }

    // @arena: before: build_now called directly in the inner loop (no build_queue), while the symbol was borrowed.
    // after: collect deps in a build queue (release the borrow), and build them.
    // This is equivalent, as the previous borrow efectively froze the depencies sets during the whole outer loop.
    pub fn build_now_dependencies(session: &mut SessionInfo, symbol: SymbolKey, step: BuildSteps) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        match symbol {
            SymbolKey::Root(_) | SymbolKey::Namespace(_) | SymbolKey::DiskDir(_) | SymbolKey::Compiled(_) | SymbolKey::Class(_) | SymbolKey::Variable(_) | SymbolKey::Function(_) => return,
            _ => {}
        }
        for step_to_build in 0..2 {
            let step_to_build = BuildSteps::from(step_to_build);
            let all_dep = st!().get_all_dependencies(symbol, step_to_build);
            if let Some(all_dep) = all_dep {
                let mut build_queue = vec![];
                for (index, dep_set) in all_dep.iter().enumerate() {
                    let dep_step = match index {
                        0 => BuildSteps::ARCH,
                        1 => BuildSteps::ARCH_EVAL,
                        _ => panic!("Unexpected step index"),
                    };
                    if let Some(dep_set) = dep_set {
                        for dep in dep_set.iter_valid(|&k| st!().contains_key(k)) {
                            build_queue.push((dep, dep_step));
                        }
                    }
                }
                for (dep, dep_step) in build_queue {
                    SyncOdoo::build_now(session, dep, dep_step);
                }
            }
            if step_to_build == step {
                break;
            }
        }
    }

    pub fn remove_from_rebuild(&mut self, symbol: SymbolKey, step: BuildSteps) {
        if DEBUG_STEPS {
            trace!("REMOVED FROM {step:?} - {}", self.symbol_table.debug_path(symbol));
        }
        if step == BuildSteps::ARCH {
            self.rebuild_arch.remove(&symbol);
        } else if step == BuildSteps::ARCH_EVAL {
            self.rebuild_arch_eval.remove(&symbol);
        } else if step == BuildSteps::VALIDATION {
            self.rebuild_validation.remove(&symbol);
        }
    }

    pub fn remove_from_rebuild_arch(&mut self, symbol: SymbolKey) {
        self.rebuild_arch.remove(&symbol);
    }

    pub fn remove_from_rebuild_arch_eval(&mut self, symbol: SymbolKey) {
        self.rebuild_arch_eval.remove(&symbol);
    }

    pub fn remove_from_rebuild_validation(&mut self, symbol: SymbolKey) {
        self.rebuild_validation.remove(&symbol);
    }

    pub fn is_in_rebuild(&self, symbol: SymbolKey, step: BuildSteps) -> bool {
        if step == BuildSteps::ARCH {
            return self.rebuild_arch.contains(&symbol);
        }
        if step == BuildSteps::ARCH_EVAL {
            return self.rebuild_arch_eval.contains(&symbol);
        }
        if step == BuildSteps::VALIDATION {
            return self.rebuild_validation.contains(&symbol);
        }
        false
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

    pub fn unload_path(session: &mut SessionInfo, path: &PathBuf, clean_cache: bool) -> Vec<SymbolKey> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let mut parents = vec![];
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_all() {
            let sym_in_data = entry.borrow().data_symbols.get(path.sanitize().as_str()).cloned();
            if let Some(sym) = sym_in_data {
                if let Some(sym) = sym.upgrade(&st!()) {
                    let parent = st!().parent(sym).unwrap();
                    if clean_cache {
                        FileMgr::delete_path(session, &path.sanitize());
                    }
                    SymbolTable::unload(session, sym);
                    parents.push(parent);
                }
                entry.borrow_mut().data_symbols.remove(path.sanitize().as_str());
                continue;
            }
            if entry.borrow().is_valid_for(path) {
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = st!().get_symbol(entry.borrow().root.into(), &tree, u32::MAX);
                if path_symbol.is_empty() {
                    continue
                }
                let path_symbol = path_symbol[0];
                let parent = st!().parent(path_symbol).unwrap();
                if clean_cache {
                    FileMgr::delete_path(session, &path.sanitize());
                    let mut to_del = st!().all_module_symbol(path_symbol);
                    let mut index = 0;
                    while index < to_del.len() {
                        // @arena: shouldn't it be using get_symbol_first_path?
                        FileMgr::delete_path(session, &st!().paths(to_del[index])[0]);
                        let mut to_del_child = st!().all_module_symbol(to_del[index]);
                        to_del.append(&mut to_del_child);
                        index += 1;
                    }
                }
                SymbolTable::unload(session, path_symbol);
                parents.push(parent);
            }
        }
        parents
    }

    /*
     * Give the symbol that is linked to the given path. As we consider that the file is opened, we do not search in entries that
     * could have it in dependencies but are not the main entry. If not found, create a new entry (is useful if the entry was dropped before
     * due to an inclusion in main entry then removed)
     */
    pub fn get_symbol_of_opened_file(session: &mut SessionInfo, path: &PathBuf) -> Option<SymbolKey> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let path_in_tree = path.to_tree_path();
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_main() {
            let sym_in_data = entry.borrow().data_symbols.get(path.sanitize().as_str()).cloned();
            if let Some(sym) = sym_in_data {
                if let Some(sym) = sym.upgrade(&st!()) {
                    return Some(sym);
                }
                continue;
            }
            if (entry.borrow().typ == EntryPointType::MAIN || entry.borrow().addon_to_odoo_path.is_some()) && entry.borrow().is_valid_for(path) {
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = st!().get_symbol(entry.borrow().root.into(), &tree, u32::MAX);
                if path_symbol.is_empty() {
                    continue;
                }
                return Some(path_symbol[0]);
            }
        }
        //Not found? Then return if it is matching a non-public entry strictly matching the file
        let mut found_an_entry = false; //there to ensure that a wrongly built entry would create infinite loop
        for entry in ep_mgr.borrow().custom_entry_points.iter() {
            let sym_in_data = entry.borrow().data_symbols.get(path.sanitize().as_str()).cloned();
            if let Some(sym) = sym_in_data {
                if let Some(sym) = sym.upgrade(&st!()) {
                    return Some(sym);
                }
                continue;
            }
            if !entry.borrow().is_public() && &path_in_tree == &PathBuf::from(&entry.borrow().path) {
                found_an_entry = true;
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = st!().get_symbol(entry.borrow().root.into(), &tree, u32::MAX);
                if path_symbol.is_empty() {
                    continue;
                }
                return Some(path_symbol[0]);
            }
        }
        for entry in ep_mgr.borrow().untitled_entry_points.iter() {
            if entry.borrow().path == path.sanitize() {
                let name = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
                let Some(file) = st!()[entry.borrow().root].module_symbols().get(name.as_str()).cloned() else {
                    continue;
                };
                return Some(file);
            }
        }
        if !found_an_entry {
            info!("Path {} not found. Creating new entry", path.to_str().expect("unable to stringify path"));
            if EntryPointMgr::create_new_custom_entry_for_path(session, &path_in_tree.sanitize(), &path.sanitize()) {
                SyncOdoo::process_rebuilds(session, false);
                return SyncOdoo::get_symbol_of_opened_file(session, path)
            }
        }
        None
    }

    /*
    * Given a path, return a tree that is valid for main entry, transformed by relational entries if necessary
     */
    pub fn path_to_main_entry_tree(&self, path: &PathBuf) -> Option<Tree> {
        for entry in self.entry_point_mgr.borrow().iter_main() {
            if (entry.borrow().typ == EntryPointType::MAIN || entry.borrow().addon_to_odoo_path.is_some()) && entry.borrow().is_valid_for(path) {
                let tree = entry.borrow().get_tree_for_entry(path);
                return Some(tree);
            }
        }
        None
    }

    pub fn is_in_workspace_or_entry(session: &mut SessionInfo, path: &str) -> bool {
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

    pub fn is_in_main_entry(session: &mut SessionInfo, path: &Vec<OYarn>) -> bool{
        path.starts_with(session.sync_odoo.main_entry_tree.as_slice())
    }

    // @arena temp
    #[allow(dead_code)]
    fn is_non_main_manifest_file(symbol_table: &SymbolTable, file_symbol: SymbolKey, file_path_buff: &PathBuf) -> bool {
        symbol_table.get_entry(file_symbol).map_or(false, |e| !e.borrow().is_main())
        && file_path_buff.components().last()
        .map_or(false, |c| c.as_os_str().to_str().map_or(false, |s| s == "__manifest__.py"))
    }

    // @arena: dead code?
    // pub fn refresh_evaluations(session: &mut SessionInfo) {
    //     let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
    //     for entry in ep_mgr.borrow().iter_all() {
    //         let mut symbols = vec![entry.borrow().root.clone()];
    //         while symbols.len() > 0 {
    //             let s = symbols.pop();
    //             if let Some(s) = s {
    //                 if s.borrow().in_workspace() && matches!(&s.borrow().typ(), SymType::FILE | SymType::PACKAGE(_)) {
    //                     session.sync_odoo.add_to_rebuild_arch_eval(s.clone());
    //                 }
    //                 if s.borrow().has_modules() {
    //                     symbols.extend(s.borrow().all_module_symbol().map(|x| {x.clone()}) );
    //                 }
    //             }
    //         }
    //     }
    //     SyncOdoo::process_rebuilds(session, false);
    // }

    pub fn get_rebuild_queue_size(&self) -> usize {
        return self.rebuild_arch.len() + self.rebuild_arch_eval.len() + self.rebuild_validation.len()
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
    pub fn get_xml_ids(session: &mut SessionInfo, from_file: SymbolKey, xml_id: &str, range: &std::ops::Range<usize>, diagnostics: &mut Vec<Diagnostic>) -> Vec<OdooData> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        if !st!().get_entry(from_file).unwrap().borrow().is_main() {
            return vec![];
        }
        let id_split = xml_id.split(".").collect::<Vec<&str>>();
        let mut module = None;
        if id_split.len() == 1 {
            // If no module name, we are in the current module
            module = st!().find_module(from_file);
        } else if id_split.len() == 2 {
            // Try to find the module by name
            if let Some(&m) = session.sync_odoo.modules.get(*id_split.first().unwrap()) {
                // @arena: to adapt, upgrade weak
                // module = m.upgrade();
                module = Weak::from(m).upgrade(&st!());
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
            return vec![];
        }
        if module.is_none() {
            warn!("Module not found for id: {}", xml_id);
            return vec![];
        }
        let module = module.unwrap();
        ModuleSymbol::get_xml_id(&st!(), module, &oyarn!("{}", id_split.last().unwrap()))
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
        let config = config.get(0);
        if config.is_none() {
            session.log_message(MessageType::ERROR, String::from("No config found for Odoo. Exiting..."));
            return Err(S!("No config found for Odoo"));
        }
        let value = config
            .and_then(|c| c.as_object())
            .and_then(|c| c.get("selectedProfile"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        return Ok(value);
    }

    pub fn send_all_configurations(session: &mut SessionInfo) {
        if let Some(ref config_file) = session.sync_odoo.config_file {
            let mut configs_map = serde_json::Map::new();
            for entry in &config_file.config {
                let html = crate::core::config::ConfigFile { config: vec![entry.clone()] }.to_html_string();
                configs_map.insert(entry.name.clone(), serde_json::Value::String(html));
            }

            configs_map.insert(
                "__all__".to_string(),
                serde_json::Value::String(config_file.to_html_string())
            );
            // Send both the HTML map and the config file as JSON
            let payload = serde_json::json!({
                "html": serde_json::Value::Object(configs_map),
                "configFile": config_file,
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
        let selected_config = match maybe_selected_config {
            None => default_profile_name(),
            Some(c) if c == "" => default_profile_name(),
            Some(config) => config,
        };
        session.sync_odoo.selected_config_profile = Some(selected_config.clone());
        if selected_config == "Disabled" {
            info!("OdooLS is disabled. Exiting...");
            return;
        }
        let config = config.and_then(|(ce, _)|{
            ce.get(&selected_config).cloned().ok_or(format!("Unable to find selected configuration \"{}\"", &selected_config))
        });
        match config {
            Ok(config) => {
                if config.abstract_ {
                    session.show_message(MessageType::ERROR, format!("Selected configuration ({}) is abstract. Please select a valid configuration and restart.", config.name));
                    return;
                }
                session.update_delay_thread_delay_duration(config.auto_refresh_delay);
                SyncOdoo::init(session, config);
                session.log_message(MessageType::LOG, format!("End building database in {} seconds. {} detected modules.",
                    (std::time::Instant::now() - start).as_secs(),
                    session.sync_odoo.modules.len()))
            },
            Err(e) => {
                session.show_message(MessageType::ERROR, format!("Unable to load config: {}  \n\nPlease select a correct profile or fix the issues in the config", e));
                error!(e);
            }
        }
    }

    pub fn register_capabilities(session: &mut SessionInfo) {
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![
                FileSystemWatcher {
                    glob_pattern: GlobPattern::String("**".to_string()),
                    kind: Some(WatchKind::Change),
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
            registrations: registrations
        };
        let result = session.send_request::<RegistrationParams, ()>(RegisterCapability::METHOD, params);
        if let Err(e) = result {
            panic!("Capabilities registration went wrong: {:?}", e);
        }
        info!("Registered Capabilities");
    }

    pub fn handle_hover(session: &mut SessionInfo, params: HoverParams) -> Result<Option<Hover>, ResponseError> {
        todo!()
        /*
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("Hover requested on {} at {} - {}",
            params.text_document_position_params.text_document.uri.to_string(),
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character));
        let path = match params.text_document_position_params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document_position_params.text_document.uri.to_string();
                if !uri.ends_with(".py") && !uri.ends_with(".xml") && !uri.ends_with(".csv") {
                    return Ok(None);
                }
                match params.text_document_position_params.text_document.uri.to_file_path(){
                    Ok(path) => path.sanitize(),
                    Err(_) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}", params.text_document_position_params.text_document.uri.to_string()),
                            data: None,
                        }
                    ),
                }
            },
            Some(schema) if schema == "untitled" => params.text_document_position_params.text_document.uri.to_string(),
            _ => return Ok(None),
        };
        let file_path_buf = PathBuf::from(path.clone());
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &file_path_buf) {
            if SyncOdoo::is_non_main_manifest_file(&file_symbol, &file_path_buf) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
            if let Some(file_info) = file_info {
                if file_info.borrow().file_info_ast.borrow().indexed_module.is_none() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
                match ast_type {
                    AstType::Python => {
                        if file_info.borrow_mut().file_info_ast.borrow().indexed_module.is_some() {
                            return Ok(HoverFeature::hover_python(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                        }
                    },
                    AstType::Xml => {
                        return Ok(HoverFeature::hover_xml(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    },
                    AstType::Csv => {
                        return Ok(HoverFeature::hover_csv(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    },
                }
            }
        }
        Ok(None)
        */
    }

    pub fn handle_goto_definition(session: &mut SessionInfo, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>, ResponseError> {
        todo!()
        /*
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("GoToDefinition requested on {} at {} - {}",
            params.text_document_position_params.text_document.uri.to_string(),
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character));
        let path = match params.text_document_position_params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document_position_params.text_document.uri.to_string();
                if !uri.ends_with(".py") && !uri.ends_with(".xml") && !uri.ends_with(".csv") {
                    return Ok(None);
                }
                match params.text_document_position_params.text_document.uri.to_file_path(){
                    Ok(path) => path.sanitize(),
                    Err(_) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}", params.text_document_position_params.text_document.uri.to_string()),
                            data: None,
                        }
                    ),
                }
            },
            Some(schema) if schema == "untitled" => params.text_document_position_params.text_document.uri.to_string(),
            _ => return Ok(None),
        };
        let file_path_buf = PathBuf::from(path.clone());
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &file_path_buf) {
            if SyncOdoo::is_non_main_manifest_file(&file_symbol, &file_path_buf) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
            if let Some(file_info) = file_info {
                if file_info.borrow().file_info_ast.borrow().indexed_module.is_none() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
                match ast_type {
                    AstType::Python => {
                        if file_info.borrow().file_info_ast.borrow().indexed_module.is_some() {
                            return Ok(DefinitionFeature::get_location(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                        }
                    },
                    AstType::Xml => {
                        return Ok(DefinitionFeature::get_location_xml(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    },
                    AstType::Csv => {
                        return Ok(DefinitionFeature::get_location_csv(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    },
                }
            }
        }
        Ok(None)
        */
    }

    pub fn handle_references(session: &mut SessionInfo, params: ReferenceParams) -> Result<Option<Vec<Location>>, ResponseError> {
        todo!()
        /*
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("References requested on {} at {} - {}",
            params.text_document_position.text_document.uri.to_string(),
            params.text_document_position.position.line,
            params.text_document_position.position.character));
        let uri = params.text_document_position.text_document.uri.to_string();
        let path = FileMgr::uri2pathname(uri.as_str());
        let file_path_buf = PathBuf::from(path.clone());
        if uri.ends_with(".py") || uri.ends_with(".pyi") || uri.ends_with(".xml") || uri.ends_with(".csv") {
            if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &file_path_buf) {
                if SyncOdoo::is_non_main_manifest_file(&file_symbol, &file_path_buf) {
                    //If the file is not in main entry, and is a manifest file, we skip it
                    return Ok(None);
                }
                let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                if let Some(file_info) = file_info {
                    if file_info.borrow().file_info_ast.borrow().indexed_module.is_none() {
                        file_info.borrow_mut().prepare_ast(session);
                    }
                    let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
                    match ast_type {
                        AstType::Python => {
                            if file_info.borrow_mut().file_info_ast.borrow().indexed_module.is_some() {
                                return Ok(ReferenceFeature::get_references(session, &file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character));
                            }
                        },
                        AstType::Xml => {
                            return Ok(ReferenceFeature::get_references_xml(session, &file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character));
                        },
                        AstType::Csv => {
                            return Ok(ReferenceFeature::get_references_csv(session, &file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character));
                        },
                    }
                }
            }
        }
        Ok(None)
        */
    }

    pub fn handle_autocomplete(session: &mut SessionInfo ,params: CompletionParams) -> Result<Option<CompletionResponse>, ResponseError> {
        todo!()
        /*
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
                if !uri.ends_with(".py") && !uri.ends_with(".xml") && !uri.ends_with(".csv") {
                    return Ok(None);
                }
                match params.text_document_position.text_document.uri.to_file_path(){
                    Ok(path) => (schema, path.sanitize()),
                    Err(_) => return Err(
                        ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!("Invalid file URI: {}", params.text_document_position.text_document.uri.to_string()),
                            data: None,
                        }
                    ),
                }
            },
            Some(schema) if schema == "untitled" => (schema, params.text_document_position.text_document.uri.to_string()),
            _ => return Ok(None)
        };
        let path_buf = PathBuf::from(path.clone());
        if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &path_buf) {
            if SyncOdoo::is_non_main_manifest_file(&file_symbol, &path_buf) {
                //If the file is not in main entry, and is a manifest file, we skip it
                return Ok(None);
            }
            let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
            if let Some(file_info) = file_info {
                if schema != "untitled" && file_info.borrow().file_info_ast.borrow().indexed_module.is_none() {
                    file_info.borrow_mut().prepare_ast(session);
                }
                if file_info.borrow_mut().file_info_ast.borrow().indexed_module.is_some() {
                    return Ok(CompletionFeature::autocomplete(session, &file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character));
                }
            }
        }
        Ok(None)
        */
    }

    pub fn handle_did_change_configuration(_session: &mut SessionInfo, _params: DidChangeConfigurationParams) {
        return;
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

    fn handle_file_update(session: &mut SessionInfo, file_uris: &Vec<Uri>) {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for uri in file_uris.iter() {
            let Ok(path) = uri.to_file_path() else {
                let msg = format!("Invalid file URI: {}", uri.to_string());
                session.log_message(MessageType::ERROR, msg.clone());
                warn!("{}", &msg);
                continue;
            };
            if Odoo::check_handle_config_file_update(session, &path) {
                continue; //config file update, handled by the config file handler
            }
            session.log_message(MessageType::INFO, format!("File update: {}", path.sanitize()));
            let file_extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let (valid, updated) = Odoo::update_file_cache(session, &path.sanitize(), file_extension, None, -100);
            if valid && updated {
                Odoo::update_file_index(session, path.clone(), file_extension, false, true);
            }
        }
    }

    pub fn handle_did_open(session: &mut SessionInfo, params: DidOpenTextDocumentParams) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        //to implement Incremental update of file caches, we have to handle DidOpen notification, to be sure
        // that we use the same base version of the file for future incrementation.
        match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                match params.text_document.uri.to_file_path(){
                    Ok(path) => {
                        let sanitized_path = path.sanitize();
                        session.log_message(MessageType::INFO, format!("File opened: {}", sanitized_path));
                        let file_extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                        let (valid, updated) = Odoo::update_file_cache(session, &sanitized_path, file_extension, Some(&vec![TextDocumentContentChangeEvent{
                            range: None,
                            range_length: None,
                            text: params.text_document.text
                        }]), params.text_document.version);
                        if valid {
                            session.sync_odoo.opened_files.push(sanitized_path.clone());
                            if session.sync_odoo.state_init == InitState::NOT_READY {
                                return
                            }
                            let tree = session.sync_odoo.path_to_main_entry_tree(&path);
                            let tree_path = path.to_tree_path();
                            if tree.is_none() ||
                            (st!().get_symbol(session.sync_odoo.get_main_entry().borrow().root.into(), tree.as_ref().unwrap(), u32::MAX).is_empty()
                            && session.sync_odoo.get_main_entry().borrow().data_symbols.get(&path.sanitize()).is_none())
                            {
                                //main entry doesn't handle this file. Let's test customs entries, or create a new one
                                let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
                                for custom_entry in ep_mgr.borrow().custom_entry_points.iter() {
                                    if custom_entry.borrow().path == tree_path.sanitize() {
                                        if updated{
                                            Odoo::update_file_index(session, path.clone(), file_extension, true, false);
                                        }
                                        return;
                                    }
                                }
                                EntryPointMgr::create_new_custom_entry_for_path(session, &tree_path.sanitize(), &sanitized_path);
                                SyncOdoo::process_rebuilds(session, false);
                            } else if updated {
                                Odoo::update_file_index(session, path.clone(), file_extension, true, false);
                            }
                        }
                    },
                    Err(_) => {
                        let msg = format!("Invalid file URI: {}", params.text_document.uri.to_string());
                        session.log_message(MessageType::ERROR, msg.clone());
                        session.show_message(MessageType::ERROR, msg.clone());
                        warn!("{}", &msg);
                        return;
                    }
                }
            },
            Some(schema) if schema == "untitled" => {
                if !["python"].contains(&params.text_document.language_id.as_str()) {
                    return; // We only handle python temporary files
                }
                let path = params.text_document.uri.to_string(); // In VSCode it is Untitled-N
                let (valid, updated) = Odoo::update_file_cache(session, &path, "py", Some(&vec![TextDocumentContentChangeEvent{
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
                        SyncOdoo::unload_path(session, &PathBuf::from(&path), false);
                    }
                }
                EntryPointMgr::create_new_untitled_entry_for_path(session, &path);
                SyncOdoo::process_rebuilds(session, false);
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
                    Err(_) => {
                        warn!("Invalid file URI: {}", params.text_document.uri.to_string());
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
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
        if let Some(file_info) = file_info {
            file_info.borrow_mut().opened = false;
            file_info.borrow_mut().version = None;
        }
        session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&PathBuf::from(path).to_tree_path().sanitize());
    }

    pub fn search_symbols_to_rebuild(session: &mut SessionInfo, path: &String) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let path_for_tree = PathBuf::from(path.clone()).to_tree_path();
        //search if the path does match a missing file path somewhere
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        let tree = session.sync_odoo.path_to_main_entry_tree(&PathBuf::from(path.clone()));
        if let Some(tree) = tree {
            if let Some(main) = ep_mgr.borrow().main_entry_point.as_ref() {
                main.borrow_mut().search_symbols_to_rebuild(session, path, &tree);
            }
        }
        for entry in ep_mgr.borrow().iter_all_but_main() {
            if entry.borrow().is_valid_for(&PathBuf::from(path)) {
                let tree = entry.borrow().get_tree_for_entry(&PathBuf::from(path.clone()));
                entry.borrow_mut().search_symbols_to_rebuild(session, path, &tree);
            }
        }
        //test if the new path is a new module
        if let Some(parent_path) = path_for_tree.parent() {
            let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
            for entry in ep_mgr.borrow().addons_entry_points.iter() {
                if entry.borrow().path == parent_path.sanitize() {
                    let module_symbol = SymbolTable::create_from_path(session, &path_for_tree, entry.borrow().get_symbol(&st!()).unwrap(), true);
                    if module_symbol.is_some() {
                        session.sync_odoo.add_to_rebuild_arch(module_symbol.unwrap());
                    }
                    break;
                }
            }
            if parent_path.sanitize() == session.sync_odoo.config.odoo_path.as_ref().unwrap_or(&"".to_string()).clone() + "/odoo/addons" {
                let addons_symbol = session.sync_odoo.get_main_entry().borrow().get_symbol(&st!()).map(|ep_sym_key|
                    st!().get_symbol(ep_sym_key, &(vec![Sy!("odoo"), Sy!("addons")], vec![]), u32::MAX)
                );
                match addons_symbol {
                    Some(addons_symbol) if !addons_symbol.is_empty() => {
                        let module_symbol = SymbolTable::create_from_path(session, &path_for_tree, addons_symbol[0], true);
                        if module_symbol.is_some() {
                            session.sync_odoo.add_to_rebuild_arch(module_symbol.unwrap());
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
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let old_path = FileMgr::uri2pathname(&f.old_uri);
            let new_path = FileMgr::uri2pathname(&f.new_uri);
            session.log_message(MessageType::INFO, format!("Renaming {} to {}", old_path, new_path));
            //1 - delete old uri
            session.sync_odoo.opened_files.retain(|x| x != &old_path.clone());
            let _ = SyncOdoo::unload_path(session, &PathBuf::from(&old_path), false);
            FileMgr::delete_path(session, &old_path);
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&old_path);
            SyncOdoo::process_rebuilds(session, false);
            //2 - create new document
            let new_path_buf = PathBuf::from(new_path.clone());
            let new_path_updated = new_path_buf.to_tree_path().sanitize();
            Odoo::search_symbols_to_rebuild(session, &new_path_updated);
            SyncOdoo::process_rebuilds(session, false);
            let tree = session.sync_odoo.path_to_main_entry_tree(&new_path_buf);
            if let Some(tree) = tree {
                if  new_path_buf.is_file() &&  st!().get_symbol(session.sync_odoo.get_main_entry().borrow().root.into(), &tree, u32::MAX).is_empty() {
                    //file has not been added to main entry. Let's build a new entry point
                    EntryPointMgr::create_new_custom_entry_for_path(session, &new_path_updated, &new_path_buf.sanitize());
                    SyncOdoo::process_rebuilds(session, false);
                }
            }
            SyncOdoo::process_rebuilds(session, false);
        }
    }

    pub fn handle_did_create(session: &mut SessionInfo, params: CreateFilesParams) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            let path_updated = PathBuf::from(path.clone()).to_tree_path().to_str().unwrap().to_string();
            session.log_message(MessageType::INFO, format!("Creating {}", path.clone()));
            Odoo::search_symbols_to_rebuild(session, &path_updated);
            session.sync_odoo.entry_point_mgr.borrow_mut().clean_entries();
        }
        SyncOdoo::process_rebuilds(session, false);
        //Now let's test if the symbol has been added to main entry tree or not
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            let path_updated = PathBuf::from(path.clone()).to_tree_path().sanitize();
            let tree = session.sync_odoo.path_to_main_entry_tree(&PathBuf::from(path.clone()));
            if PathBuf::from(&path).is_file() && (tree.is_none() || st!().get_symbol(session.sync_odoo.get_main_entry().borrow().root.into(), &tree.unwrap(), u32::MAX).is_empty()) {
                //file has not been added to main entry. Let's build a new entry point
                EntryPointMgr::create_new_custom_entry_for_path(session, &path_updated, &path);
                SyncOdoo::process_rebuilds(session, false);
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
            let _ = SyncOdoo::unload_path(session, &PathBuf::from(&path), false);
            FileMgr::delete_path(session, &path);
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&PathBuf::from(path).to_tree_path().sanitize());
        }
        SyncOdoo::process_rebuilds(session, false);
    }

    pub fn handle_did_change(session: &mut SessionInfo, params: DidChangeTextDocumentParams) {
        let (scheme, path) = match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                match params.text_document.uri.to_file_path(){
                    Ok(path) => (schema, path.sanitize()),
                    Err(_) => {
                        warn!("Invalid file URI: {}", params.text_document.uri.to_string());
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
        let path_buf = PathBuf::from(&path);
        session.log_message(MessageType::INFO, format!("File changed: {}", path));
        let file_extension = match scheme.as_str() {
            "file" => path_buf.extension().clone().and_then(|s| s.to_str()).unwrap_or(""),
            "untitled" => "py",
            _ => return,
        };
        let (valid, updated) = Odoo::update_file_cache(session, &path, file_extension, Some(&params.content_changes), params.text_document.version);
        if session.sync_odoo.state_init != InitState::NOT_READY && valid && updated {
            Odoo::update_file_index(session, path_buf.clone(), file_extension, false, false);
        }
    }

    pub fn handle_did_save(session: &mut SessionInfo, params: DidSaveTextDocumentParams) {
        let Ok(path) = params.text_document.uri.to_file_path() else {
            let msg = format!("Invalid file URI: {}", params.text_document.uri.to_string());
            session.log_message(MessageType::ERROR, msg.clone());
            warn!("{}", &msg);
            return;
        };
        if Odoo::check_handle_config_file_update(session, &path) {
            return; //config file update, handled by the config file handler
        }
        session.log_message(MessageType::INFO, format!("File saved: {}", path.sanitize()));
        return;
        //No need to update the index on save, as the file change event will do it
        //Odoo::update_file_index(session, path,true, false, false);
    }

    // return (valid, updated) booleans
    // if the file has been updated, is valid for an index reload, and contents have been changed
    fn update_file_cache(session: &mut SessionInfo, path: &String, extension: &str, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: i32) -> (bool, bool) {
        if ["py", "xml", "csv"].contains(&extension) || Odoo::is_config_workspace_file(session, &PathBuf::from(path)){
            session.log_message(MessageType::INFO, format!("File Change Event: {}, version {}", path, version));
            let (file_updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path, content, Some(version), false);
            file_info.borrow_mut().publish_diagnostics(session); //To push potential syntax errors or refresh previous one
            return (!file_info.borrow().opened || version >= 0, file_updated);
        }
        (false, false)
    }

    pub fn update_file_index(session: &mut SessionInfo, path: PathBuf, extension: &str, _is_open: bool, force_delay: bool) {
        if ["py", "xml", "csv"].contains(&extension) || Odoo::is_config_workspace_file(session, &path){
            SessionInfo::request_update_file_index(session, &path, force_delay);
        }
    }

    pub(crate) fn handle_document_symbols(session: &mut SessionInfo<'_>, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>, ResponseError> {
        todo!();
        /*
        session.log_message(MessageType::INFO, format!("Document symbol requested for {}",
            params.text_document.uri.as_str(),
        ));
        let (schema, path) = match params.text_document.uri.scheme().map(|scheme| scheme.to_lowercase()) {
            Some(schema) if schema == "file" => {
                let uri = params.text_document.uri.to_string();
                if !uri.ends_with(".py") && !uri.ends_with(".pyi") && !uri.ends_with(".xml") && !uri.ends_with(".csv") {
                    return Ok(None);
                }
                match params.text_document.uri.to_file_path(){
                    Ok(path) => (schema, path.sanitize()),
                    Err(_) => {
                        warn!("Invalid file URI: {}", params.text_document.uri.to_string());
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
            if schema != "untitled" && file_info.borrow().file_info_ast.borrow().indexed_module.is_none() {
                file_info.borrow_mut().prepare_ast(session);
            }
            return Ok(DocumentSymbolFeature::get_symbols(session, &file_info));
        }
        Ok(None)
         */
    }

    pub fn handle_workspace_symbols(session: &mut SessionInfo<'_>, params: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>, ResponseError> {
        todo!()
        /*
        session.log_message(MessageType::INFO, format!("Workspace Symbol requested with query {}",
            params.query,
        ));
        WorkspaceSymbolFeature::get_workspace_symbols(session, params.query)
         */
    }

    pub fn handle_workspace_symbols_resolve(session: &mut SessionInfo<'_>, symbol: WorkspaceSymbol) -> Result<WorkspaceSymbol, ResponseError> {
        todo!()
        /*
        session.log_message(MessageType::INFO, format!("Workspace Symbol Resolve for symbol {}",
            symbol.name,
        ));
        WorkspaceSymbolFeature::resolve_workspace_symbol(session, &symbol)
         */
    }

    /// Checks if the given path is a configuration file under one of the workspace folders.
    fn is_config_workspace_file(session: &mut SessionInfo, path: &PathBuf) -> bool {
        session.sync_odoo
        .get_file_mgr()
        .borrow()
        .get_processed_workspace_folders()
        .iter()
        .any(|(_, ws_dir)| path.starts_with(ws_dir) && path.ends_with("odools.toml"))
    }

    /// Checks if the given path is a configuration file and handles the update accordingly.
    /// Returns true if the path is a configuration file and was handled, false otherwise.
    fn check_handle_config_file_update(session: &mut SessionInfo, path: &PathBuf) -> bool {
        // Check if the change is affecting a config file
        if Odoo::is_config_workspace_file(session, path) {
            let config_result = config::get_configuration(session)
                .and_then(|(cfg_map, cfg_file)| {
                    let config_name = session.sync_odoo.selected_config_profile.clone().unwrap_or(default_profile_name());
                    cfg_map.get(&config_name)
                        .cloned()
                        .ok_or_else(|| format!("Unable to find selected configuration \"{config_name}\""))
                        .map(|config| (config, cfg_file))
                });

            match config_result {
                Ok((new_config, cfg_file)) => {
                    if config::needs_restart(&session.sync_odoo.config, &new_config) {
                        // Changes require a restart, ask the client to restart the server
                        session.send_notification("$Odoo/restartNeeded", ());
                    } else {
                        // Changes can be applied without restart
                        session.sync_odoo.config_file = Some(cfg_file);
                        session.sync_odoo.config = new_config;
                        // Recalculate diagnostic filters
                        session.sync_odoo.get_file_mgr().borrow_mut().update_all_file_diagnostic_filters(session);
                        session.update_delay_thread_delay_duration(session.sync_odoo.config.auto_refresh_delay);
                    }
                }
                Err(err) => {
                    // Invalid config, send a notification to the user and add the error to the logs
                    let msg = format!("Invalid configuration file: {err}.");
                    error!("{msg}");
                    session.show_message(MessageType::ERROR, msg);
                }
            }
            true
        }
        else {
            false
        }
    }

}
