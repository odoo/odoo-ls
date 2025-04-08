use crate::core::config::Config;
use crate::core::entry_point::EntryPointType;
use crate::features::document_symbols::DocumentSymbolFeature;
use crate::threads::SessionInfo;
use crate::features::completion::CompletionFeature;
use crate::features::definition::DefinitionFeature;
use crate::features::hover::HoverFeature;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use byteyarn::{yarn, Yarn};
use lsp_server::ResponseError;
use lsp_types::*;
use request::{RegisterCapability, Request, WorkspaceConfiguration};
use ruff_python_parser::{Mode, ParseOptions};
use tracing::{debug, error, warn, info, trace};

use std::collections::HashSet;
use weak_table::PtrWeakHashSet;
use std::process::Command;
use std::str::FromStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use std::cmp;
use regex::Regex;
use crate::{constants::*, oyarn, Sy};
use super::config::{DiagMissingImportsMode, RefreshMode};
use super::entry_point::{EntryPoint, EntryPointMgr};
use super::file_mgr::FileMgr;
use super::import_resolver::ImportCache;
use super::symbols::disk_dir_symbol::DiskDirSymbol;
use super::symbols::symbol::Symbol;
use crate::core::model::Model;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::core::python_arch_eval::PythonArchEval;
use crate::core::python_odoo_builder::PythonOdooBuilder;
use crate::core::python_validator::PythonValidator;
use crate::utils::{PathSanitizer, ToFilePath as _};
use crate::S;
//use super::python_arch_builder::PythonArchBuilder;

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
    pub config: Config,
    pub entry_point_mgr: Rc<RefCell<EntryPointMgr>>, //An Rc to be able to clone it and free session easily
    pub has_main_entry:bool,
    pub has_odoo_main_entry: bool,
    pub has_valid_python: bool,
    pub main_entry_tree: Vec<OYarn>,
    pub stubs_dirs: Vec<String>,
    pub stdlib_dir: String,
    file_mgr: Rc<RefCell<FileMgr>>,
    pub modules: HashMap<OYarn, Weak<RefCell<Symbol>>>,
    pub models: HashMap<OYarn, Rc<RefCell<Model>>>,
    pub interrupt_rebuild: Arc<AtomicBool>,
    pub terminate_rebuild: Arc<AtomicBool>,
    pub watched_file_updates: Arc<AtomicU32>,
    rebuild_arch: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_arch_eval: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_validation: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub state_init: InitState,
    pub must_reload_paths: Vec<(Weak<RefCell<Symbol>>, String)>,
    pub load_odoo_addons: bool, //indicate if we want to load odoo addons or not
    pub need_rebuild: bool, //if true, the next process_rebuilds will drop everything and rebuild everything
    pub import_cache: Option<ImportCache>,
    pub capabilities: lsp_types::ClientCapabilities,
    pub opened_files: Vec<String>,
}

unsafe impl Send for SyncOdoo {}

impl SyncOdoo {

    pub fn new() -> Self {
        let sync_odoo = Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            config: Config::new(),
            entry_point_mgr: Rc::new(RefCell::new(EntryPointMgr::new())),
            has_main_entry: false,
            has_odoo_main_entry: false,
            has_valid_python: false,
            main_entry_tree: vec![],
            file_mgr: Rc::new(RefCell::new(FileMgr::new())),
            stubs_dirs: vec![env::current_dir().unwrap().join("typeshed").join("stubs").sanitize(),
            env::current_dir().unwrap().join("additional_stubs").sanitize()],
            stdlib_dir: env::current_dir().unwrap().join("typeshed").join("stdlib").sanitize(),
            modules: HashMap::new(),
            models: HashMap::new(),
            interrupt_rebuild: Arc::new(AtomicBool::new(false)),
            terminate_rebuild: Arc::new(AtomicBool::new(false)),
            watched_file_updates: Arc::new(AtomicU32::new(0)),
            rebuild_arch: PtrWeakHashSet::new(),
            rebuild_arch_eval: PtrWeakHashSet::new(),
            rebuild_validation: PtrWeakHashSet::new(),
            state_init: InitState::NOT_READY,
            must_reload_paths: vec![],
            load_odoo_addons: true,
            need_rebuild: false,
            import_cache: None,
            capabilities: lsp_types::ClientCapabilities::default(),
            opened_files: vec![]
        };
        sync_odoo
    }

    pub fn reset(session: &mut SessionInfo, config: Config) {
        session.log_message(MessageType::INFO, S!("Resetting Database..."));
        info!("Resetting database...");
        session.sync_odoo.version_major = 0;
        session.sync_odoo.version_minor = 0;
        session.sync_odoo.version_micro = 0;
        session.sync_odoo.full_version = "0.0.0".to_string();
        session.sync_odoo.config = Config::new();
        FileMgr::clear(session);//only reset files, as workspace folders didn't change
        session.sync_odoo.stubs_dirs = vec![env::current_dir().unwrap().join("typeshed").join("stubs").sanitize(),
            env::current_dir().unwrap().join("additional_stubs").sanitize()];
        session.sync_odoo.stdlib_dir = env::current_dir().unwrap().join("typeshed").join("stdlib").sanitize();
        session.sync_odoo.modules = HashMap::new();
        session.sync_odoo.models = HashMap::new();
        session.sync_odoo.rebuild_arch = PtrWeakHashSet::new();
        session.sync_odoo.rebuild_arch_eval = PtrWeakHashSet::new();
        session.sync_odoo.rebuild_validation = PtrWeakHashSet::new();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.sync_odoo.load_odoo_addons = true;
        session.sync_odoo.need_rebuild = false;
        session.sync_odoo.watched_file_updates = Arc::new(AtomicU32::new(0));
        //drop all entries, except entries of opened files
        session.sync_odoo.entry_point_mgr.borrow_mut().reset_entry_points(false);
        SyncOdoo::init(session, config);
    }

    pub fn init(session: &mut SessionInfo, config: Config) {
        info!("Initializing odoo");
        let start_time = Instant::now();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.send_notification("$Odoo/loadingStatusUpdate", "start");
        session.sync_odoo.config = config;
        if session.sync_odoo.config.no_typeshed {
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
        {
            session.sync_odoo.entry_point_mgr.borrow_mut().add_entry_to_builtins(session.sync_odoo.stdlib_dir.clone());
            for stub_dir in session.sync_odoo.stubs_dirs.clone().iter() {
                session.sync_odoo.entry_point_mgr.borrow_mut().add_entry_to_public(stub_dir.clone());
            }
            let output = Command::new(session.sync_odoo.config.python_path.clone()).args(&["-c", "import sys; import json; print(json.dumps(sys.path))"]).output();
            if let Err(_output) = &output {
                error!("Wrong python command: {}", session.sync_odoo.config.python_path.clone());
                session.send_notification("$Odoo/invalid_python_path", ());
                session.send_notification("$Odoo/loadingStatusUpdate", "stop");
                return;
            }
            session.sync_odoo.has_valid_python = true;
            let output = output.unwrap();
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
                        session.sync_odoo.entry_point_mgr.borrow_mut().add_entry_to_public( final_path.clone());
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("{}", stderr);
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
        let path = PathBuf::from(&session.sync_odoo.stdlib_dir);
        let builtins_path = path.join("builtins.pyi");
        if !builtins_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find builtins.pyi. Are you sure that typeshed has been downloaded. If you are building from source, make sure to initialize submodules with 'git submodule init' and 'git submodule update'."));
            error!("Unable to find builtins at: {}", builtins_path.sanitize());
            return false;
        };
        let tree_builtins = path.to_tree();
        let entry_stdlib = session.sync_odoo.find_stdlib_entry_point();
        let disk_dir_builtins = entry_stdlib.borrow().root.borrow().get_symbol(&tree_builtins, u32::MAX);
        if disk_dir_builtins.is_empty() {
            panic!("Unable to find builtins disk dir symbol");
        }
        let _builtins_rc_symbol = Symbol::create_from_path(session, &builtins_path, disk_dir_builtins[0].clone(), false);
        session.sync_odoo.add_to_rebuild_arch(_builtins_rc_symbol.unwrap());
        SyncOdoo::process_rebuilds(session)
    }

    pub fn build_database(session: &mut SessionInfo) {
        session.log_message(MessageType::INFO, String::from("Building Database"));
        let result = SyncOdoo::build_base(session);
        if result {
            SyncOdoo::build_modules(session);
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
                let re = Regex::new(r#"version_info = \((['\"]?(\D+~)?\d+['\"]?, \d+, \d+, \w+, \d+, \D+)\)"#).unwrap();
                let result = re.captures(line);
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
        let odoo_path = session.sync_odoo.config.odoo_path.clone();
        let Some(odoo_path) = odoo_path else {
            info!("Odoo path not provided. Continuing in single file mode");
            return false;
        };
        session.sync_odoo.has_main_entry = true;
        let odoo_sym = session.sync_odoo.entry_point_mgr.borrow_mut().set_main_entry(odoo_path.clone());
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
        odoo_sym.borrow_mut().set_is_external(false);
        let odoo_odoo = Symbol::create_from_path(session, &config_odoo_path.join("odoo"), odoo_sym.clone(), false);
        if odoo_odoo.is_none() {
            panic!("Not able to find odoo with given path. Aborting...");
        }
        let odoo_typ = odoo_odoo.as_ref().unwrap().borrow().typ().clone();
        match odoo_typ {
            SymType::PACKAGE(PackageType::PYTHON_PACKAGE) => {
                odoo_odoo.as_ref().unwrap().borrow_mut().as_python_package_mut().self_import = true;
                session.sync_odoo.add_to_rebuild_arch(odoo_odoo.as_ref().unwrap().clone());
            },
            SymType::NAMESPACE => {
                //starting from > 18.0, odoo is now a namespace. Start import project from odoo/__main__.py
                let main_file = Symbol::create_from_path(session, &PathBuf::from(config_odoo_path.clone()).join("odoo").join("__main__.py"),  odoo_odoo.as_ref().unwrap().clone(), false);
                if main_file.is_none() {
                    panic!("Not able to find odoo/__main__.py. Aborting...");
                }
                main_file.as_ref().unwrap().borrow_mut().as_file_mut().self_import = true;
                session.sync_odoo.add_to_rebuild_arch(main_file.unwrap());
            },
            _ => panic!("Root symbol is not a package or namespace (> 18.0)")
        }
        session.sync_odoo.has_odoo_main_entry = true; // set it now has we need it to parse base addons
        if !SyncOdoo::process_rebuilds(session){
            return false;
        }
        //search common odoo addons path
        let mut addon_symbol = session.sync_odoo.get_symbol(&odoo_path.clone(), &tree(vec!["odoo", "addons"], vec![]), u32::MAX);
        if addon_symbol.is_empty() {
            let odoo = session.sync_odoo.get_symbol(&odoo_path, &tree(vec!["odoo"], vec![]), u32::MAX);
            if odoo.is_empty() {
                session.log_message(MessageType::WARNING, "Odoo not found. Switching to non-odoo mode...".to_string());
                session.sync_odoo.has_odoo_main_entry = false;
                return false;
            }
            //if we are > 18.1, odoo.addons is not imported automatically anymore. Let's try to import it manually
            let addons_folder = Symbol::create_from_path(session, &PathBuf::from(config_odoo_path).join("odoo").join("addons"), odoo_odoo.as_ref().unwrap().clone(), false);
            if let Some(addons) = addons_folder {
                addon_symbol = vec![addons];
            } else {
                session.log_message(MessageType::WARNING, "Not able to find odoo/addons. Please check your configuration. Switching to non-odoo mode...".to_string());
                session.sync_odoo.has_odoo_main_entry = false;
                return false;
            }
        }
        let addon_symbol = addon_symbol[0].clone();
        if odoo_addon_path.exists() {
            if session.sync_odoo.load_odoo_addons {
                addon_symbol.borrow_mut().add_path(
                    odoo_addon_path.sanitize()
                );
                session.sync_odoo.entry_point_mgr.borrow_mut().add_entry_to_addons(odoo_addon_path.sanitize(),
                    Some(odoo_entry.clone()),
                    Some(vec![Sy!("odoo"),
                        Sy!("addons")]));
            }
        } else {
            session.log_message(MessageType::WARNING, format!("Unable to find odoo addons path at {}. You can ignore this message if you use a nightly build or if your community addons are in another addon paths.", odoo_addon_path.sanitize()));
        }
        for addon in session.sync_odoo.config.addons.clone().iter() {
            let addon_path = PathBuf::from(addon);
            if addon_path.exists() {
                addon_symbol.borrow_mut().add_path(
                    addon_path.sanitize()
                );
                session.sync_odoo.entry_point_mgr.borrow_mut().add_entry_to_addons(addon.clone(),
                    Some(odoo_entry.clone()),
                    Some(vec![Sy!("odoo"),
                        Sy!("addons")]));
            }
        }
        return true;
    }

    fn build_modules(session: &mut SessionInfo) {
        {
            let addons_symbol = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &tree(vec!["odoo", "addons"], vec![]), u32::MAX)[0].clone();
            let addons_path = addons_symbol.borrow().paths().clone();
            for addon_path in addons_path.iter() {
                info!("searching modules in {}", addon_path);
                if PathBuf::from(addon_path).exists() {
                    //browse all dir in path
                    for item in PathBuf::from(addon_path).read_dir().expect("Unable to browse and odoo addon directory") {
                        match item {
                            Ok(item) => {
                                if item.file_type().unwrap().is_dir() && !session.sync_odoo.modules.contains_key(&oyarn!("{}", item.file_name().to_str().unwrap())) {
                                    let module_symbol = Symbol::create_from_path(session, &item.path(), addons_symbol.clone(), true);
                                    if module_symbol.is_some() {
                                        session.sync_odoo.add_to_rebuild_arch(module_symbol.unwrap());
                                    }
                                }
                            },
                            Err(_) => {}
                        }
                    }
                }
            }
        }
        if !SyncOdoo::process_rebuilds(session){
            return;
        }
        //println!("{}", self.symbols.as_ref().unwrap().borrow_mut().debug_print_graph());
        //fs::write("out_architecture.json", self.get_symbol(&tree(vec!["odoo", "addons", "module_1"], vec![])).as_ref().unwrap().borrow().debug_to_json().to_string()).expect("Unable to write file");
        let modules_count = session.sync_odoo.modules.len();
        info!("End building modules. {} modules loaded", modules_count);
        session.log_message(MessageType::INFO, format!("End building modules. {} modules loaded", modules_count));
        session.sync_odoo.state_init = InitState::ODOO_READY;
    }

    //search for a symbol with a tree local to an unknown entrypoint
    pub fn get_symbol(&self, from_path: &str, tree: &Tree, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        //find which entrypoint to use
        for entry in self.entry_point_mgr.borrow().iter_all() {
            let entry_point = entry.borrow();
            if entry_point.is_public() || from_path.starts_with(&entry_point.path) {
                let symbols = entry_point.root.borrow().get_symbol(&(entry_point.addon_to_odoo_tree.as_ref().unwrap_or(&entry_point.tree).iter().chain(&tree.0).map(|x| x.clone()).collect(), tree.1.clone()), position);
                if !symbols.is_empty() {
                    return symbols;
                }
            }
        }
        //no valid entry point? that's wrong, an entry shoud have been created
        warn!("Unable to find symbol for path: {}", from_path);
        vec![]
    }

    pub fn get_main_entry(&self) -> Rc<RefCell<EntryPoint>> {
        return self.entry_point_mgr.borrow().main_entry_point.as_ref().expect("Unable to find main entry point").clone()
    }

    fn pop_item(&mut self, step: BuildSteps) -> Option<Rc<RefCell<Symbol>>> {
        let mut arc_sym: Option<Rc<RefCell<Symbol>>> = None;
        //Part 1: Find the symbol with a unmutable set
        {
            let set =  match step {
                BuildSteps::ARCH_EVAL => &self.rebuild_arch_eval,
                BuildSteps::VALIDATION => &self.rebuild_validation,
                _ => &self.rebuild_arch
            };
            let mut selected_sym: Option<Rc<RefCell<Symbol>>> = None;
            let mut selected_count: u32 = 999999999;
            let mut current_count: u32;
            for sym in set {
                current_count = 0;
                let file = sym.borrow().get_file().unwrap().upgrade().unwrap();
                let file = file.borrow();
                let all_dep = file.get_all_dependencies(step);
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
                                dep_set.iter().filter(|dep| index_set.contains(dep)).count() as u32;
                        }
                    }
                }
                if current_count < selected_count {
                    selected_sym = Some(sym.clone());
                    selected_count = current_count;
                    if current_count == 0 {
                        break;
                    }
                }
            }
            if selected_sym.is_some() {
                arc_sym = selected_sym.map(|x| x.clone());
            }
        }
        {
            let set =  match step {
                BuildSteps::ARCH_EVAL => &mut self.rebuild_arch_eval,
                BuildSteps::VALIDATION => &mut self.rebuild_validation,
                _ => &mut self.rebuild_arch
            };
            if arc_sym.is_none() {
                set.clear(); //remove any potential dead weak ref
                return None;
            }
            let arc_sym_unwrapped = arc_sym.unwrap();
            if !set.remove(&arc_sym_unwrapped) {
                panic!("Unable to remove selected symbol from rebuild set")
            }
            return Some(arc_sym_unwrapped);
        }
    }

    fn add_from_self_reload(session: &mut SessionInfo) {
        for (weak_sym, path) in session.sync_odoo.must_reload_paths.clone().iter() {
            if let Some(parent) = weak_sym.upgrade() {
                let in_addons = parent.borrow().get_main_entry_tree(session) == tree(vec!["odoo", "addons"], vec![]);
                let new_symbol = Symbol::create_from_path(session, &PathBuf::from(path), parent, in_addons);
                if new_symbol.is_some() {
                    let new_symbol = new_symbol.as_ref().unwrap().clone();
                    new_symbol.borrow_mut().set_is_external(false);
                    let new_sym_typ = new_symbol.borrow().typ();
                    match new_sym_typ {
                        SymType::PACKAGE(PackageType::PYTHON_PACKAGE) => {
                            new_symbol.borrow_mut().as_python_package_mut().self_import = true;
                        },
                        SymType::FILE => {
                            new_symbol.borrow_mut().as_file_mut().self_import = true;
                        },
                        SymType::PACKAGE(PackageType::MODULE) => {},
                        _ => {panic!("Unexpected symbol type: {:?}", new_sym_typ);}
                    }
                    if matches!(new_symbol.borrow().typ(), SymType::PACKAGE(PackageType::MODULE)) {
                        session.sync_odoo.modules.insert(new_symbol.borrow().name().clone(), Rc::downgrade(&new_symbol));
                    }
                    session.sync_odoo.must_reload_paths.retain(|x| !Weak::ptr_eq(&x.0, weak_sym));
                    session.sync_odoo.add_to_rebuild_arch(new_symbol.clone());
                }
            }
        }
    }

    pub fn process_rebuilds(session: &mut SessionInfo) -> bool {
        session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
        SyncOdoo::add_from_self_reload(session);
        session.sync_odoo.import_cache = Some(ImportCache{ modules: HashMap::new(), main_modules: HashMap::new() });
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_validation_rebuilt: HashSet<Tree> = HashSet::new();
        trace!("Starting rebuild: {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_validation.len());
        while !session.sync_odoo.need_rebuild && (!session.sync_odoo.rebuild_arch.is_empty() || !session.sync_odoo.rebuild_arch_eval.is_empty() || !session.sync_odoo.rebuild_validation.is_empty()) {
            if DEBUG_THREADS {
                trace!("remains: {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_validation.len());
            }
            if session.sync_odoo.terminate_rebuild.load(Ordering::SeqCst){
                info!("Terminating rebuilds due to server shutdown");
                return false;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::ARCH);
            if let Some(sym_rc) = sym {
                let (tree, entry) = sym_rc.borrow().get_tree_and_entry();
                if already_arch_rebuilt.contains(&tree) {
                    info!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                let mut builder = PythonArchBuilder::new(entry.unwrap(), sym_rc);
                builder.load_arch(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::ARCH_EVAL);
            if let Some(sym_rc) = sym {
                let (tree, entry) = sym_rc.borrow().get_tree_and_entry();
                if already_arch_eval_rebuilt.contains(&tree) {
                    info!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                let mut builder = PythonArchEval::new(entry.unwrap(), sym_rc);
                builder.eval_arch(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::VALIDATION);
            if let Some(sym_rc) = sym {
                let (tree, entry) = sym_rc.borrow_mut().get_tree_and_entry();
                if already_validation_rebuilt.contains(&tree) {
                    info!("Already validation rebuilt, skipping");
                    continue;
                }
                already_validation_rebuilt.insert(tree);
                if session.sync_odoo.state_init == InitState::ODOO_READY && session.sync_odoo.interrupt_rebuild.load(Ordering::SeqCst) {
                    session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
                    session.log_message(MessageType::INFO, S!("Rebuild interrupted"));
                    session.request_delayed_rebuild();
                    session.sync_odoo.add_to_validations(sym_rc.clone());
                    return true;
                }
                let mut validator = PythonValidator::new(entry.unwrap(), sym_rc);
                validator.validate(session);
                continue;
            }
        }
        if session.sync_odoo.need_rebuild {
            session.log_message(MessageType::INFO, S!("Rebuild required. Resetting database on breaktime..."));
            SessionInfo::request_reload(session);
        }
        session.sync_odoo.import_cache = None;
        trace!("Leaving rebuild with remaining tasks: {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_validation.len());
        true
    }

    pub fn add_to_rebuild_arch(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if DEBUG_THREADS {
            trace!("ADDED TO ARCH - {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
        }
        if symbol.borrow().build_status(BuildSteps::ARCH) != BuildStatus::IN_PROGRESS {
            let sym_clone = symbol.clone();
            let mut sym_borrowed = sym_clone.borrow_mut();
            sym_borrowed.set_build_status(BuildSteps::ARCH, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_arch.insert(symbol);
        }
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if DEBUG_THREADS {
            trace!("ADDED TO EVAL - {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
        }
        if symbol.borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS {
            let sym_clone = symbol.clone();
            let mut sym_borrowed = sym_clone.borrow_mut();
            sym_borrowed.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_arch_eval.insert(symbol);
        }
    }

    pub fn add_to_validations(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if DEBUG_THREADS {
            trace!("ADDED TO VALIDATION - {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
        }
        if symbol.borrow().build_status(BuildSteps::VALIDATION) != BuildStatus::IN_PROGRESS {
            symbol.borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_validation.insert(symbol);
        }
    }

    /* Ask for an immediate rebuild of the given symbol if possible.
    return true if a rebuild has been done
     */
    pub fn build_now(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, step: BuildSteps) -> bool {
        match symbol.borrow().typ() {
            SymType::ROOT | SymType::NAMESPACE | SymType::DISK_DIR | SymType::COMPILED | SymType::CLASS | SymType::VARIABLE => return false,
            _ => {}
        }
        if DEBUG_REBUILD_NOW {
            if symbol.borrow().build_status(step) == BuildStatus::INVALID {
                panic!("Trying to build an invalid symbol: {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
            }
            if symbol.borrow().build_status(step) == BuildStatus::IN_PROGRESS && !session.sync_odoo.is_in_rebuild(&symbol, step) {
                error!("Trying to build a symbol that is NOT in the queue: {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
            }
        }
        if symbol.borrow().build_status(step) == BuildStatus::PENDING && (step == BuildSteps::ARCH || session.sync_odoo.is_in_rebuild(&symbol, step)) {
            SyncOdoo::build_now_dependencies(session, symbol, step);
            let entry_point = symbol.borrow().get_entry().unwrap();
            session.sync_odoo.remove_from_rebuild(&symbol, step);
            if step == BuildSteps::ARCH {
                let mut builder = PythonArchBuilder::new(entry_point, symbol.clone());
                builder.load_arch(session);
                return true;
            } else if step == BuildSteps::ARCH_EVAL {
                if DEBUG_REBUILD_NOW {
                    if symbol.borrow().build_status(BuildSteps::ARCH) != BuildStatus::DONE {
                        panic!("An evaluation has been requested on a non-arched symbol: {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
                    }
                }
                let mut builder = PythonArchEval::new(entry_point, symbol.clone());
                builder.eval_arch(session);
                return true;
            } else if step == BuildSteps::VALIDATION {
                if DEBUG_REBUILD_NOW {
                    if symbol.borrow().build_status(BuildSteps::ARCH) != BuildStatus::DONE || symbol.borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::DONE {
                        panic!("An evaluation has been requested on a non-arched symbol: {}", symbol.borrow().paths().first().unwrap_or(&symbol.borrow().name().to_string()));
                    }
                }
                let mut validator = PythonValidator::new(entry_point, symbol.clone());
                validator.validate(session);
                return true;
            }
        }
        false
    }

    pub fn build_now_dependencies(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, step: BuildSteps) {
        let symbol = symbol.borrow();
        match symbol.typ() {
            SymType::ROOT | SymType::NAMESPACE | SymType::DISK_DIR | SymType::COMPILED | SymType::CLASS | SymType::VARIABLE | SymType::FUNCTION => return,
            _ => {}
        }
        for step_to_build in 0..2 {
            let step_to_build = BuildSteps::from(step_to_build);
            let all_dep = symbol.get_all_dependencies(step_to_build);
            if let Some(all_dep) = all_dep {
                for (index, dep_set) in all_dep.iter().enumerate() {
                    let dep_step = match index {
                        0 => BuildSteps::ARCH,
                        1 => BuildSteps::ARCH_EVAL,
                        _ => panic!("Unexpected step index"),
                    };
                    if let Some(dep_set) = dep_set {
                        for dep in dep_set.iter() {
                            SyncOdoo::build_now(session, &dep, dep_step);
                        }
                    }
                }
            }
            if step_to_build == step {
                break;
            }
        }
    }

    pub fn remove_from_rebuild(&mut self, symbol: &Rc<RefCell<Symbol>>, step: BuildSteps) {
        if step == BuildSteps::ARCH {
            self.rebuild_arch.remove(symbol);
        } else if step == BuildSteps::ARCH_EVAL {
            self.rebuild_arch_eval.remove(symbol);
        } else if step == BuildSteps::VALIDATION {
            self.rebuild_validation.remove(symbol);
        }
    }

    pub fn remove_from_rebuild_arch(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch.remove(symbol);
    }

    pub fn remove_from_rebuild_arch_eval(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch_eval.remove(symbol);
    }

    pub fn remove_from_rebuild_validation(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_validation.remove(symbol);
    }

    pub fn is_in_rebuild(&self, symbol: &Rc<RefCell<Symbol>>, step: BuildSteps) -> bool {
        if step == BuildSteps::ARCH {
            return self.rebuild_arch.contains(symbol);
        }
        if step == BuildSteps::ARCH_EVAL {
            return self.rebuild_arch_eval.contains(symbol);
        }
        if step == BuildSteps::VALIDATION {
            return self.rebuild_validation.contains(symbol);
        }
        false
    }

    pub fn get_file_mgr(&self) -> Rc<RefCell<FileMgr>> {
        self.file_mgr.clone()
    }

    pub fn _unload_path(session: &mut SessionInfo, path: &PathBuf, clean_cache: bool) -> Vec<Rc<RefCell<Symbol>>> {
        let mut parents = vec![];
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_all() {
            if entry.borrow().is_valid_for(path.sanitize().as_str()) {
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = entry.borrow().root.borrow().get_symbol(&tree, u32::MAX);
                if path_symbol.is_empty() {
                    continue
                }
                let path_symbol = path_symbol[0].clone();
                let parent = path_symbol.borrow().parent().clone().unwrap().upgrade().unwrap();
                if clean_cache {
                    FileMgr::delete_path(session, &path.sanitize());
                    let mut to_del = Vec::from_iter(path_symbol.borrow().all_module_symbol().map(|x| x.clone()));
                    let mut index = 0;
                    while index < to_del.len() {
                        FileMgr::delete_path(session, &to_del[index].borrow().paths()[0]);
                        let mut to_del_child = Vec::from_iter(to_del[index].borrow().all_module_symbol().map(|x| x.clone()));
                        to_del.append(&mut to_del_child);
                        index += 1;
                    }
                }
                Symbol::unload(session, path_symbol.clone());
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
    pub fn get_symbol_of_opened_file(session: &mut SessionInfo, path: &PathBuf) -> Option<Rc<RefCell<Symbol>>> {
        for entry in session.sync_odoo.entry_point_mgr.borrow().iter_for_import(session.sync_odoo.entry_point_mgr.borrow().main_entry_point.as_ref().unwrap()) {
            if (entry.borrow().typ == EntryPointType::MAIN || entry.borrow().addon_to_odoo_path.is_some()) && entry.borrow().is_valid_for(path.as_os_str().to_str().unwrap()) {
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = entry.borrow().root.borrow().get_symbol(&tree, u32::MAX);
                if path_symbol.is_empty() {
                    continue;
                }
                return Some(path_symbol[0].clone());
            }
        }
        //Not found? Then return if it is matching a non-public entry strictly matching the file
        let mut found_an_entry = false; //there to ensure that a wrongly built entry would create infinite loop
        for entry in session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter() {
            if !entry.borrow().is_public() && path == &PathBuf::from(&entry.borrow().path) {
                found_an_entry = true;
                let tree = entry.borrow().get_tree_for_entry(path);
                let path_symbol = entry.borrow().root.borrow().get_symbol(&tree, u32::MAX);
                if path_symbol.is_empty() {
                    continue;
                }
                return Some(path_symbol[0].clone());
            }
        }
        if !found_an_entry {
            info!("Path {} not found. Creating new entry", path.to_str().expect("unable to stringify path"));
            EntryPointMgr::create_new_custom_entry_for_path(session, &path.sanitize());
            SyncOdoo::process_rebuilds(session);
            return SyncOdoo::get_symbol_of_opened_file(session, path)
        }
        None
    }

    /*
    * Given a path, return a tree that is valid for main entry, transformed by relational entries if necessary
     */
    pub fn path_to_main_entry_tree(&self, path: &PathBuf) -> Option<Tree> {
        for entry in self.entry_point_mgr.borrow().iter_main() {
            if (entry.borrow().typ == EntryPointType::MAIN || entry.borrow().addon_to_odoo_path.is_some()) && entry.borrow().is_valid_for(path.sanitize().as_str()) {
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

    pub fn refresh_evaluations(session: &mut SessionInfo) {
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_all() {
            let mut symbols = vec![entry.borrow().root.clone()];
            while symbols.len() > 0 {
                let s = symbols.pop();
                if let Some(s) = s {
                    if s.borrow().in_workspace() && matches!(&s.borrow().typ(), SymType::FILE | SymType::PACKAGE(_)) {
                        session.sync_odoo.add_to_rebuild_arch_eval(s.clone());
                    }
                    symbols.extend(s.borrow().all_module_symbol().map(|x| {x.clone()}) );
                }
            }
        }
        SyncOdoo::process_rebuilds(session);
    }

    pub fn get_rebuild_queue_size(&self) -> usize {
        return self.rebuild_arch.len() + self.rebuild_arch_eval.len() + self.rebuild_validation.len()
    }

    pub fn load_capabilities(&mut self, capabilities: &lsp_types::ClientCapabilities) {
        info!("Client capabilities: {:?}", capabilities);
        self.capabilities = capabilities.clone();
    }

}

#[derive(Debug)]
pub struct Odoo {}

impl Odoo {

    fn update_configuration(session: &mut SessionInfo) -> Result<Config, String> {
        let configuration_item = ConfigurationItem{
            scope_uri: None,
            section: Some("Odoo".to_string()),
        };
        let config_params = ConfigurationParams {
            items: vec![configuration_item],
        };
        let config = match session.send_request::<ConfigurationParams, Vec<serde_json::Value>>(WorkspaceConfiguration::METHOD, config_params) {
            Ok(config) => config.unwrap(),
            Err(_) => {
                return Err(S!("Unable to get configuration from client, client not available"));
            }
        };
        let config = config.get(0);
        if !config.is_some() {
            session.log_message(MessageType::ERROR, String::from("No config found for Odoo. Exiting..."));
            return Err(S!("no config found for Odoo"));
        }
        let config = config.unwrap();
        //values for sync block
        let mut _refresh_mode : RefreshMode = RefreshMode::OnSave;
        let mut _auto_save_delay : u64 = 2000;
        let mut _ac_filter_model_names : bool = true;
        let mut _diag_missing_imports : DiagMissingImportsMode = DiagMissingImportsMode::All;
        let mut selected_configuration: String = S!("");
        let mut configurations = serde_json::Map::new();
        if let Some(map) = config.as_object() {
            for (key, value) in map {
                match key.as_str() {
                    "autoRefresh" => {
                        if let Some(refresh_mode) = value.as_str() {
                            _refresh_mode = match RefreshMode::from_str(refresh_mode) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    session.log_message(MessageType::ERROR, String::from("Unable to parse RefreshMode. Setting it to onSave"));
                                    RefreshMode::OnSave
                                }
                            };
                        }
                    },
                    "autoRefreshDelay" => {
                        if let Some(refresh_delay) = value.as_u64() {
                            _auto_save_delay = std::cmp::max(refresh_delay, 1000);
                        } else {
                            session.log_message(MessageType::ERROR, String::from("Unable to parse auto_save_delay. Setting it to 2000"));
                            _auto_save_delay = 2000
                        }
                    },
                    "autocompletion" => {
                        if let Some(autocompletion_config) = value.as_object() {
                            for (key, value) in autocompletion_config {
                                match key.as_str() {
                                    "filterModelNames" =>{
                                        if let Some(ac_filter_model_names) = value.as_bool() {
                                            _ac_filter_model_names = ac_filter_model_names;
                                        } else {
                                            session.log_message(MessageType::ERROR, String::from("Unable to parse autocompletion.ac_filter_model_names . Setting it to true"));
                                        }
                                    }
                                    _ => {
                                        session.log_message(MessageType::ERROR, format!("Unknown autocompletion config key: autocompletion.{}", key));
                                    },
                                }
                            }
                        } else {
                            session.log_message(MessageType::ERROR, String::from("Unable to parse autocompletion_config"));
                        }
                    },
                    "diagMissingImportLevel" => {
                        if let Some(diag_import_level) = value.as_str() {
                            _diag_missing_imports = match DiagMissingImportsMode::from_str(diag_import_level) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    session.log_message(MessageType::ERROR, String::from("Unable to parse diag_import_level. Setting it to all"));
                                    DiagMissingImportsMode::All
                                }
                            };
                        }
                    },
                    "configurations" => {
                        if let Some(values)= value.as_object() {
                            configurations = values.clone();
                        }
                    },
                    "selectedConfiguration" => {
                        if let Some(value_str) = value.as_str() {
                            selected_configuration = value_str.to_string();
                        }
                    },
                    "serverLogLevel" | "disablePythonLanguageServerPopup" => {
                        // Too late, set it with command line
                        // disablePythonLanguageServerPopup, does not affect us, pass
                    },
                    _ => {
                        session.log_message(MessageType::ERROR, format!("Unknown config key: {}", key));
                    },
                }
            }
        }
        debug!("configurations: {:?}", configurations);
        debug!("selected_configuration: {:?}", selected_configuration);
        let mut config = Config::new();
        if configurations.contains_key(&selected_configuration) {
            let odoo_conf = configurations.get(&selected_configuration).unwrap();
            let odoo_conf = odoo_conf.as_object().unwrap();
            config.addons = odoo_conf.get("validatedAddonsPaths").expect("An odoo config must contains a addons value")
                .as_array().expect("the addons value must be an array")
                .into_iter().map(|v| v.as_str().unwrap().to_string()).collect();
            let odoo_path = odoo_conf.get("odooPath");
            if let Some(odoo_path) = odoo_path {
                config.odoo_path = Some(odoo_path.as_str().expect("odooPath must be a String").to_string());
            }
            if let Some(python_path) = odoo_conf.get("finalPythonPath") {
                if python_path.is_string() {
                    config.python_path = python_path.as_str().unwrap().to_string();
                } else {
                    session.log_message(MessageType::ERROR, String::from("pythonPath must be a string, using 'python' as pythonPath"));
                    config.python_path = S!("python");
                }
            } else {
                session.log_message(MessageType::ERROR, String::from("pythonPath must be defined, using 'python' as pythonPath"));
                config.python_path = S!("python");
            }
            if let Some(additional_stubs) = odoo_conf.get("additional_stubs"){
                config.additional_stubs = additional_stubs.as_array().expect("additional_stubs must be an Array")
                .into_iter().map(|v| v.as_str().expect("additional_stubs values must be strings").to_string()).collect();
            }
        } else {
            config.addons = vec![];
            config.odoo_path = None;
            session.log_message(MessageType::ERROR, S!("Unable to find selected configuration. No odoo path has been found."));
        }
        config.refresh_mode = _refresh_mode;
        config.auto_save_delay = _auto_save_delay;
        config.ac_filter_model_names = _ac_filter_model_names;
        config.diag_missing_imports = _diag_missing_imports;

        debug!("Final config: {:?}", config);
        Ok(config)
    }

    pub fn init(session: &mut SessionInfo) {
        let start = std::time::Instant::now();
        session.log_message(MessageType::LOG, String::from("Building new Odoo knowledge database"));
        let config = Odoo::update_configuration(session);
        match config {
            Ok(config) => {
                SyncOdoo::init(session, config);
                session.log_message(MessageType::LOG, format!("End building database in {} seconds. {} detected modules.",
                    (std::time::Instant::now() - start).as_secs(),
                    session.sync_odoo.modules.len()))
            },
            Err(e) => {
                session.log_message(MessageType::ERROR, format!("Unable to load config: {}", e));
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
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("Hover requested on {} at {} - {}",
            params.text_document_position_params.text_document.uri.to_string(),
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character));
        let path = FileMgr::uri2pathname(params.text_document_position_params.text_document.uri.as_str());
        if params.text_document_position_params.text_document.uri.to_string().ends_with(".py") || 
        params.text_document_position_params.text_document.uri.to_string().ends_with(".pyi") {
            if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &PathBuf::from(path.clone())) {
                let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                if let Some(file_info) = file_info {
                    if file_info.borrow().ast.is_some() {
                        return Ok(HoverFeature::get_hover(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn handle_goto_definition(session: &mut SessionInfo, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>, ResponseError> {
        if session.sync_odoo.state_init == InitState::NOT_READY {
            return Ok(None);
        }
        session.log_message(MessageType::INFO, format!("GoToDefinition requested on {} at {} - {}",
            params.text_document_position_params.text_document.uri.to_string(),
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character));
        let path = FileMgr::uri2pathname(params.text_document_position_params.text_document.uri.as_str());
        if params.text_document_position_params.text_document.uri.to_string().ends_with(".py") ||
        params.text_document_position_params.text_document.uri.to_string().ends_with(".pyi") {
            if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &PathBuf::from(path.clone())) {
                let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                if let Some(file_info) = file_info {
                    if file_info.borrow().ast.is_some() {
                        return Ok(DefinitionFeature::get_location(session, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character));
                    }
                }
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
        let path = FileMgr::uri2pathname(params.text_document_position.text_document.uri.as_str());
        if params.text_document_position.text_document.uri.to_string().ends_with(".py") {
            if let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, &PathBuf::from(path.clone())) {
                let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                if let Some(file_info) = file_info {
                    if file_info.borrow().ast.is_some() {
                        return Ok(CompletionFeature::autocomplete(session, &file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character));
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn handle_did_change_configuration(session: &mut SessionInfo, _params: DidChangeConfigurationParams) {
        let old_config = session.sync_odoo.config.clone();
        match Odoo::update_configuration(session) {
            Ok (config) => {
                session.sync_odoo.config = config.clone();
                if config.odoo_path != old_config.odoo_path ||
                    config.addons != old_config.addons ||
                    config.additional_stubs != old_config.additional_stubs ||
                    config.stdlib != old_config.stdlib ||
                    config.python_path != old_config.python_path {
                        SyncOdoo::reset(session, config);
                } else {
                    if old_config.diag_missing_imports != session.sync_odoo.config.diag_missing_imports {
                        SyncOdoo::refresh_evaluations(session);
                    }
                    if old_config.auto_save_delay != session.sync_odoo.config.auto_save_delay {
                        session.update_auto_refresh_delay(session.sync_odoo.config.auto_save_delay);
                    }
                }
            },
            Err(e) => {
                session.log_message(MessageType::ERROR, format!("Unable to update config: {}", e));
                error!("Unable to update configuration: {}", e);
            }
        }
    }

    pub fn handle_did_change_workspace_folders(session: &mut SessionInfo, params: DidChangeWorkspaceFoldersParams) {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let mut file_mgr = file_mgr.borrow_mut();
        for added in params.event.added {
            file_mgr.add_workspace_folder(added.uri.to_string());
        }
        for removed in params.event.removed {
            file_mgr.remove_workspace_folder(removed.uri.to_string());
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
        if session.sync_odoo.config.refresh_mode == RefreshMode::Off || session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for uri in file_uris.iter() {
            let path = uri.to_file_path().unwrap();
            session.log_message(MessageType::INFO, format!("File update: {}", path.sanitize()));
            let (valid, updated) = Odoo::update_file_cache(session, path.clone(), None, -100);
            if valid && updated {
                Odoo::update_file_index(session, path, true, false, true);
            }
        }
    }

    pub fn handle_did_open(session: &mut SessionInfo, params: DidOpenTextDocumentParams) {
        //to implement Incremental update of file caches, we have to handle DidOpen notification, to be sure
        // that we use the same base version of the file for future incrementation.
        if let Ok(path) = params.text_document.uri.to_file_path() { //temp file has no file path
            session.log_message(MessageType::INFO, format!("File opened: {}", path.sanitize()));
            let (valid, updated) = Odoo::update_file_cache(session, path.clone(), Some(&vec![TextDocumentContentChangeEvent{
                range: None,
                range_length: None,
                    text: params.text_document.text}]), params.text_document.version);
            if valid {
                session.sync_odoo.opened_files.push(path.sanitize());
                if session.sync_odoo.config.refresh_mode == RefreshMode::Off || session.sync_odoo.state_init == InitState::NOT_READY {
                    return
                }
                let tree = session.sync_odoo.path_to_main_entry_tree(&path);
                if tree.is_none() || session.sync_odoo.get_main_entry().borrow().root.borrow().get_symbol(tree.as_ref().unwrap(), u32::MAX).is_empty() {
                    //main entry doesn't handle this file. Let's test customs entries, or create a new one
                    let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
                    for custom_entry in ep_mgr.borrow().custom_entry_points.iter() {
                        if custom_entry.borrow().path == path.sanitize() {
                            if updated{
                                Odoo::update_file_index(session, path,true, true, false);
                            }
                            return;
                        }
                    }
                    EntryPointMgr::create_new_custom_entry_for_path(session, &path.sanitize());
                    SyncOdoo::process_rebuilds(session);
                } else if updated {
                    Odoo::update_file_index(session, path,true, true, false);
                }
            }
        }
    }

    pub fn handle_did_close(session: &mut SessionInfo, params: DidCloseTextDocumentParams) {
        if let Ok(path) = params.text_document.uri.to_file_path() {
            session.log_message(MessageType::INFO, format!("File closed: {}", path.sanitize()));
            session.sync_odoo.opened_files.retain(|x| x != &path.sanitize());
            let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path.to_str().unwrap().to_string());
            if let Some(file_info) = file_info {
                file_info.borrow_mut().opened = false;
            }
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&path.sanitize());
        }
    }

    pub fn search_symbols_to_rebuild(session: &mut SessionInfo, path: &String) {
        //search if the path does match a missing file path somewhere
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        for entry in ep_mgr.borrow().iter_main() {
            if entry.borrow().is_valid_for(path.as_str()) {
                let tree = entry.borrow().get_tree_for_entry(&PathBuf::from(path.clone()));
                entry.borrow_mut().search_symbols_to_rebuild(session, &tree);
            }
        }
        for entry in ep_mgr.borrow().iter_all_but_main() {
            if entry.borrow().is_valid_for(path.as_str()) {
                let tree = entry.borrow().get_tree_for_entry(&PathBuf::from(path.clone()));
                entry.borrow_mut().search_symbols_to_rebuild(session, &tree);
            }
        }
        //test if the new path is a new module
        if let Some(parent_path) = PathBuf::from(path).parent() {
            let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
            for entry in ep_mgr.borrow().addons_entry_points.iter() {
                if entry.borrow().path == parent_path.sanitize() {
                    let module_symbol = Symbol::create_from_path(session, &PathBuf::from(path), entry.borrow().get_symbol().unwrap().clone(), true);
                    if module_symbol.is_some() {
                        session.sync_odoo.add_to_rebuild_arch(module_symbol.unwrap());
                    }
                    break;
                }
            }
            if parent_path.sanitize() == session.sync_odoo.config.odoo_path.as_ref().unwrap_or(&"".to_string()).clone() + "/odoo/addons" {
                let addons_symbol = session.sync_odoo.get_main_entry().borrow().root.clone().borrow().get_symbol(&(vec![Sy!("odoo"), Sy!("addons")], vec![]), u32::MAX);
                if !addons_symbol.is_empty() {
                    let module_symbol = Symbol::create_from_path(session, &PathBuf::from(path), addons_symbol[0].clone(), true);
                    if module_symbol.is_some() {
                        session.sync_odoo.add_to_rebuild_arch(module_symbol.unwrap());
                    }
                } else {
                    error!("Unable to find addons symbol to create new module");
                }
            }
        }
    }

    pub fn handle_did_rename(session: &mut SessionInfo, params: RenameFilesParams) {
        if session.sync_odoo.config.refresh_mode == RefreshMode::Off || session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let old_path = FileMgr::uri2pathname(&f.old_uri);
            let new_path = FileMgr::uri2pathname(&f.new_uri);
            session.log_message(MessageType::INFO, format!("Renaming {} to {}", old_path, new_path));
            //1 - delete old uri
            session.sync_odoo.opened_files.retain(|x| x != &old_path.clone());
            let _ = SyncOdoo::_unload_path(session, &PathBuf::from(&old_path), false);
            FileMgr::delete_path(session, &old_path);
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&old_path);
            SyncOdoo::process_rebuilds(session);
            //2 - create new document
            Odoo::search_symbols_to_rebuild(session, &new_path);
            SyncOdoo::process_rebuilds(session);
            let tree = session.sync_odoo.entry_point_mgr.borrow().tree_for_main(&new_path);
            if let Some(tree) = tree {
                if session.sync_odoo.get_main_entry().borrow().root.borrow().get_symbol(&tree, u32::MAX).is_empty() {
                    //file has not been added to main entry. Let's build a new entry point
                    EntryPointMgr::create_new_custom_entry_for_path(session, &new_path);
                    SyncOdoo::process_rebuilds(session);
                }
            }
            SyncOdoo::process_rebuilds(session);
        }
    }

    pub fn handle_did_create(session: &mut SessionInfo, params: CreateFilesParams) {
        if session.sync_odoo.config.refresh_mode == RefreshMode::Off || session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            session.log_message(MessageType::INFO, format!("Creating {}", path.clone()));
            Odoo::search_symbols_to_rebuild(session, &path);
            session.sync_odoo.entry_point_mgr.borrow_mut().clean_entries();
        }
        SyncOdoo::process_rebuilds(session);
        //Now let's test if the symbol has been added to main entry tree or not
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            let tree = PathBuf::from(path.clone()).to_tree();
            if session.sync_odoo.get_main_entry().borrow().root.borrow().get_symbol(&tree, u32::MAX).is_empty() {
                //file has not been added to main entry. Let's build a new entry point
                EntryPointMgr::create_new_custom_entry_for_path(session, &path);
                SyncOdoo::process_rebuilds(session);
            }
        }
    }

    pub fn handle_did_delete(session: &mut SessionInfo, params: DeleteFilesParams) {
        if session.sync_odoo.config.refresh_mode == RefreshMode::Off || session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        for f in params.files.iter() {
            let path = FileMgr::uri2pathname(&f.uri);
            session.log_message(MessageType::INFO, format!("Deleting {}", path));
            //1 - delete old uri
            let _ = SyncOdoo::_unload_path(session, &PathBuf::from(&path), false);
            FileMgr::delete_path(session, &path);
            session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&path);
        }
        SyncOdoo::process_rebuilds(session);
    }

    pub fn handle_did_change(session: &mut SessionInfo, params: DidChangeTextDocumentParams) {
        if let Ok(path) = params.text_document.uri.to_file_path() {
            session.log_message(MessageType::INFO, format!("File changed: {}", path.sanitize()));
            let version = params.text_document.version;
            let (valid, updated) = Odoo::update_file_cache(session, path.clone(), Some(&params.content_changes), version);
            if valid && updated {
                if (matches!(session.sync_odoo.config.refresh_mode, RefreshMode::Off | RefreshMode::OnSave)) || session.sync_odoo.state_init == InitState::NOT_READY {
                    return
                }
                Odoo::update_file_index(session, path, false, false, false);
            }
        }
    }

    pub fn handle_did_save(session: &mut SessionInfo, params: DidSaveTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();
        session.log_message(MessageType::INFO, format!("File saved: {}", path.sanitize()));
        if session.sync_odoo.config.refresh_mode != RefreshMode::OnSave || session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        //Before dropping the cache and reload file content, let's be sure that the new content contains a valid AST. Else, we would prefer to use previous one.
        let text_rope = match fs::read_to_string(path.clone()) {
            Ok(content) => {
                ropey::Rope::from(content.as_str())
            },
            Err(_) => {
                session.log_message(MessageType::ERROR, format!("Failed to read file {}", path.to_str().unwrap_or("[invalid path]")));
                return;
            },
        };
        let content = text_rope.slice(..);
        let source = content.to_string(); //cast to string to get a version with all changes
        let ast = ruff_python_parser::parse_unchecked(source.as_str(), ParseOptions::from(Mode::Module));
        if ast.errors().is_empty() {
            Odoo::update_file_index(session, path,true, false, false);
        }
    }

    // return (valid, updated) booleans
    // if the file has been updated, is valid for an index reload, and contents have been changed
    fn update_file_cache(session: &mut SessionInfo, path: PathBuf, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: i32) -> (bool, bool) {
        if path.extension().is_some() && path.extension().unwrap() == "py" {
            session.log_message(MessageType::INFO, format!("File Change Event: {}, version {}", path.to_str().unwrap(), version));
            let (file_updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &path.sanitize(), content, Some(version), false);
            file_info.borrow_mut().publish_diagnostics(session); //To push potential syntax errors or refresh previous one
            return (file_info.borrow().valid && (!file_info.borrow().opened || version >= 0), file_updated);
        }
        (false, false)
    }

    pub fn update_file_index(session: &mut SessionInfo, path: PathBuf, is_save: bool, is_open: bool, force_delay: bool) {
        if path.extension().is_some() && path.extension().unwrap() == "py" {
            SessionInfo::request_update_file_index(session, &path, is_save, force_delay);
        }
    }

    pub(crate) fn handle_document_symbols(session: &mut SessionInfo<'_>, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>, ResponseError> {
        session.log_message(MessageType::INFO, format!("Document symbol requested for {}",
            params.text_document.uri.as_str(),
        ));
        let path = FileMgr::uri2pathname(params.text_document.uri.as_str());
        if params.text_document.uri.to_string().ends_with(".py") || params.text_document.uri.to_string().ends_with(".pyi") {
            let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path);
            if let Some(file_info) = file_info {
                if file_info.borrow().ast.is_some() {
                    return Ok(DocumentSymbolFeature::get_symbols(session, &file_info));
                }
            }
        }
        Ok(None)
    }

}
