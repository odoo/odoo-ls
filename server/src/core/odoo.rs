use crate::core::config::{Config, PythonPathRequest, PythonPathRequestResult};
use crate::threads::SessionInfo;
use crate::features::completion::CompletionFeature;
use crate::features::definition::DefinitionFeature;
use crate::features::hover::HoverFeature;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use lsp_server::ResponseError;
use lsp_types::*;
use request::{RegisterCapability, Request, WorkspaceConfiguration};
use tracing::{debug, error, info, trace};

use std::collections::HashSet;
use weak_table::PtrWeakHashSet;
use std::process::Command;
use std::str::FromStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use std::cmp;
use regex::Regex;
use crate::constants::*;
use super::config::{DiagMissingImportsMode, RefreshMode};
use super::file_mgr::FileMgr;
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
    pub symbols: Option<Rc<RefCell<Symbol>>>,
    pub stubs_dirs: Vec<String>,
    pub stdlib_dir: String,
    file_mgr: Rc<RefCell<FileMgr>>,
    pub modules: HashMap<String, Weak<RefCell<Symbol>>>,
    pub models: HashMap<String, Rc<RefCell<Model>>>,
    pub interrupt_rebuild: Arc<AtomicBool>,
    rebuild_arch: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_arch_eval: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_odoo: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_validation: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub state_init: InitState,
    pub not_found_symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub must_reload_paths: Vec<(Weak<RefCell<Symbol>>, String)>,
    pub load_odoo_addons: bool, //indicate if we want to load odoo addons or not
    pub need_rebuild: bool //if true, the next process_rebuilds will drop everything and rebuild everything
}

unsafe impl Send for SyncOdoo {}

impl SyncOdoo {

    pub fn new() -> Self {
        let symbols = Symbol::new_root();
        symbols.borrow_mut().as_root_mut().weak_self = Some(Rc::downgrade(&symbols)); // manually set weakself for root symbols
        let sync_odoo = Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            config: Config::new(),
            symbols: Some(symbols),
            file_mgr: Rc::new(RefCell::new(FileMgr::new())),
            stubs_dirs: vec![env::current_dir().unwrap().join("typeshed").join("stubs").sanitize(),
            env::current_dir().unwrap().join("additional_stubs").sanitize()],
            stdlib_dir: env::current_dir().unwrap().join("typeshed").join("stdlib").sanitize(),
            modules: HashMap::new(),
            models: HashMap::new(),
            interrupt_rebuild: Arc::new(AtomicBool::new(false)),
            rebuild_arch: PtrWeakHashSet::new(),
            rebuild_arch_eval: PtrWeakHashSet::new(),
            rebuild_odoo: PtrWeakHashSet::new(),
            rebuild_validation: PtrWeakHashSet::new(),
            state_init: InitState::NOT_READY,
            not_found_symbols: PtrWeakHashSet::new(),
            must_reload_paths: vec![],
            load_odoo_addons: true,
            need_rebuild: false,
        };
        sync_odoo
    }

    pub fn reset(session: &mut SessionInfo, config: Config) {
        let symbols = Symbol::new_root();
        session.log_message(MessageType::INFO, S!("Resetting Database..."));
        info!("Resetting database...");
        session.sync_odoo.version_major = 0;
        session.sync_odoo.version_minor = 0;
        session.sync_odoo.version_micro = 0;
        session.sync_odoo.full_version = "0.0.0".to_string();
        session.sync_odoo.config = Config::new();
        session.sync_odoo.symbols = Some(symbols);
        session.sync_odoo.file_mgr.clone().borrow_mut().clear(session);//only reset files, as workspace folders didn't change
        session.sync_odoo.stubs_dirs = vec![env::current_dir().unwrap().join("typeshed").join("stubs").sanitize(),
            env::current_dir().unwrap().join("additional_stubs").sanitize()];
        session.sync_odoo.stdlib_dir = env::current_dir().unwrap().join("typeshed").join("stdlib").sanitize();
        session.sync_odoo.modules = HashMap::new();
        session.sync_odoo.models = HashMap::new();
        session.sync_odoo.rebuild_arch = PtrWeakHashSet::new();
        session.sync_odoo.rebuild_arch_eval = PtrWeakHashSet::new();
        session.sync_odoo.rebuild_odoo = PtrWeakHashSet::new();
        session.sync_odoo.rebuild_validation = PtrWeakHashSet::new();
        session.sync_odoo.state_init = InitState::NOT_READY;
        session.sync_odoo.not_found_symbols = PtrWeakHashSet::new();
        session.sync_odoo.load_odoo_addons = true;
        session.sync_odoo.need_rebuild = false;
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
            let mut root_symbol = session.sync_odoo.symbols.as_ref().unwrap().borrow_mut();
            root_symbol.add_path(session.sync_odoo.stdlib_dir.clone());
            for stub_dir in session.sync_odoo.stubs_dirs.iter() {
                root_symbol.add_path(stub_dir.clone());
            }
            let output = Command::new(session.sync_odoo.config.python_path.clone()).args(&["-c", "import sys; import json; print(json.dumps(sys.path))"]).output();
            if let Err(_output) = &output {
                error!("Wrong python command: {}", session.sync_odoo.config.python_path.clone());
                session.send_notification("$Odoo/invalid_python_path", ());
                session.send_notification("$Odoo/loadingStatusUpdate", "stop");
                return;
            }
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
                        root_symbol.add_path(final_path.clone());
                        root_symbol.as_root_mut().sys_path.push(final_path.clone());
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("{}", stderr);
            }
        }
        SyncOdoo::load_builtins(session);
        session.sync_odoo.state_init = InitState::PYTHON_READY;
        SyncOdoo::build_database(session);
        session.send_notification("$Odoo/loadingStatusUpdate", "stop");
        info!("Time taken: {} ms", start_time.elapsed().as_millis());
    }

    pub fn load_builtins(session: &mut SessionInfo) {
        let path = PathBuf::from(&session.sync_odoo.stdlib_dir);
        let builtins_path = path.join("builtins.pyi");
        if !builtins_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find builtins.pyi"));
            error!("Unable to find builtins at: {}", builtins_path.sanitize());
            return;
        };
        let _builtins_rc_symbol = Symbol::create_from_path(session, &builtins_path, session.sync_odoo.symbols.as_ref().unwrap().clone(), false);
        session.sync_odoo.add_to_rebuild_arch(_builtins_rc_symbol.unwrap());
        SyncOdoo::process_rebuilds(session);
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
        let release_path = PathBuf::from(odoo_path.clone()).join("odoo/release.py");
        let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
        if !release_path.exists() {
            session.log_message(MessageType::ERROR, String::from("Unable to find release.py - Aborting"));
            return false;
        }
        let (_version_major, _version_minor, _version_micro) = SyncOdoo::read_version(session, release_path);
        if _version_major == 0 {
            return false;
        }
        let _full_version = format!("{}.{}.{}", _version_major, _version_minor, _version_micro);
        session.log_message(MessageType::INFO, format!("Odoo version: {}", _full_version));
        if _version_major < 14 {
            session.log_message(MessageType::ERROR, String::from("Odoo version is less than 14. The tool only supports version 14 and above. Aborting..."));
            return false;
        }
        session.sync_odoo.version_major = _version_major;
        session.sync_odoo.version_minor = _version_minor;
        session.sync_odoo.version_micro = _version_micro;
        session.sync_odoo.full_version = _full_version;
        //build base
        session.sync_odoo.symbols.as_ref().unwrap().borrow_mut().add_path(session.sync_odoo.config.odoo_path.clone());
        if session.sync_odoo.symbols.is_none() {
            panic!("Odoo root symbol not found")
        }
        let root_symbol = session.sync_odoo.symbols.as_ref().unwrap().clone();
        let config_odoo_path = session.sync_odoo.config.odoo_path.clone();
        let added_symbol = Symbol::create_from_path(session, &PathBuf::from(config_odoo_path).join("odoo"),  root_symbol.clone(), false);
        added_symbol.as_ref().unwrap().borrow_mut().as_python_package_mut().self_import = true;
        session.sync_odoo.add_to_rebuild_arch(added_symbol.unwrap());
        SyncOdoo::process_rebuilds(session);
        //search common odoo addons path
        let addon_symbol = session.sync_odoo.get_symbol(&tree(vec!["odoo", "addons"], vec![]), u32::MAX);
        if addon_symbol.is_empty() {
            let odoo = session.sync_odoo.get_symbol(&tree(vec!["odoo"], vec![]), u32::MAX);
            if odoo.is_empty() {
                panic!("Not able to find odoo. Please check your configuration");
            }
            panic!("Not able to find odoo/addons. Please check your configuration");
        }
        let addon_symbol = addon_symbol[0].clone();
        if odoo_addon_path.exists() {
            if session.sync_odoo.load_odoo_addons {
                addon_symbol.borrow_mut().add_path(
                    odoo_addon_path.sanitize()
                );
            }
        } else {
            let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
            session.log_message(MessageType::ERROR, format!("Unable to find odoo addons path at {}", odoo_addon_path.sanitize()));
            return false;
        }
        for addon in session.sync_odoo.config.addons.iter() {
            let addon_path = PathBuf::from(addon);
            if addon_path.exists() {
                addon_symbol.borrow_mut().add_path(
                    addon_path.sanitize()
                );
            }
        }
        return true;
    }

    fn build_modules(session: &mut SessionInfo) {
        {
            let addons_symbol = session.sync_odoo.get_symbol(&tree(vec!["odoo", "addons"], vec![]), u32::MAX)[0].clone();
            let addons_path = addons_symbol.borrow_mut().paths().clone();
            for addon_path in addons_path.iter() {
                info!("searching modules in {}", addon_path);
                if PathBuf::from(addon_path).exists() {
                    //browse all dir in path
                    for item in PathBuf::from(addon_path).read_dir().expect("Unable to find odoo addons path") {
                        match item {
                            Ok(item) => {
                                if item.file_type().unwrap().is_dir() && !session.sync_odoo.modules.contains_key(&item.file_name().to_str().unwrap().to_string()) {
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
        SyncOdoo::process_rebuilds(session);
        //println!("{}", self.symbols.as_ref().unwrap().borrow_mut().debug_print_graph());
        //fs::write("out_architecture.json", self.get_symbol(&tree(vec!["odoo", "addons", "module_1"], vec![])).as_ref().unwrap().borrow().debug_to_json().to_string()).expect("Unable to write file");
        let modules_count = session.sync_odoo.modules.len();
        info!("End building modules. {} modules loaded", modules_count);
        session.log_message(MessageType::INFO, format!("End building modules. {} modules loaded", modules_count));
        session.sync_odoo.state_init = InitState::ODOO_READY;
    }

    pub fn get_symbol(&self, tree: &Tree, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        self.symbols.as_ref().unwrap().borrow_mut().get_symbol(&tree, position)
    }

    fn pop_item(&mut self, step: BuildSteps) -> Option<Rc<RefCell<Symbol>>> {
        let mut arc_sym: Option<Rc<RefCell<Symbol>>> = None;
        //Part 1: Find the symbol with a unmutable set
        {
            let set =  match step {
                BuildSteps::ARCH_EVAL => &self.rebuild_arch_eval,
                BuildSteps::ODOO => &self.rebuild_odoo,
                BuildSteps::VALIDATION => &self.rebuild_validation,
                _ => &self.rebuild_arch
            };
            let mut selected_sym: Option<Rc<RefCell<Symbol>>> = None;
            let mut selected_count: u32 = 999999999;
            let mut current_count: u32;
            for sym in &*set {
                current_count = 0;
                let file = sym.borrow().get_file().unwrap().upgrade().unwrap();
                let file = file.borrow();
                for (index, dep_set) in file.get_all_dependencies(step).iter().enumerate() {
                    let index_set =  match index {
                        x if x == BuildSteps::ARCH as usize => &self.rebuild_arch,
                        x if x == BuildSteps::ARCH_EVAL as usize => &self.rebuild_arch_eval,
                        x if x == BuildSteps::VALIDATION as usize => &self.rebuild_validation,
                        _ => continue,
                    };
                    current_count +=
                        dep_set.iter().filter(|dep| index_set.contains(dep)).count() as u32;
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
                BuildSteps::ODOO => &mut self.rebuild_odoo,
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
                let in_addons = parent.borrow().get_tree() == tree(vec!["odoo", "addons"], vec![]);
                let new_symbol = Symbol::create_from_path(session, &PathBuf::from(path), parent, in_addons);
                if new_symbol.is_some() {
                    let new_symbol = new_symbol.as_ref().unwrap().clone();
                    if matches!(new_symbol.borrow().typ(), SymType::PACKAGE(PackageType::MODULE)) {
                        session.sync_odoo.modules.insert(new_symbol.borrow().name().clone(), Rc::downgrade(&new_symbol));
                    }
                    session.sync_odoo.add_to_rebuild_arch(new_symbol.clone());
                }
            }
        }
    }

    pub fn process_rebuilds(session: &mut SessionInfo) {
        session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
        SyncOdoo::add_from_self_reload(session);
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_odoo_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_validation_rebuilt: HashSet<Tree> = HashSet::new();
        while !session.sync_odoo.need_rebuild && (!session.sync_odoo.rebuild_arch.is_empty() || !session.sync_odoo.rebuild_arch_eval.is_empty() || !session.sync_odoo.rebuild_odoo.is_empty() || !session.sync_odoo.rebuild_validation.is_empty()) {
            trace!("remains: {:?} - {:?} - {:?} - {:?}", session.sync_odoo.rebuild_arch.len(), session.sync_odoo.rebuild_arch_eval.len(), session.sync_odoo.rebuild_odoo.len(), session.sync_odoo.rebuild_validation.len());
            let sym = session.sync_odoo.pop_item(BuildSteps::ARCH);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow().get_tree();
                if already_arch_rebuilt.contains(&tree) {
                    info!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchBuilder::new(sym_rc);
                builder.load_arch(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::ARCH_EVAL);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow().get_tree();
                if already_arch_eval_rebuilt.contains(&tree) {
                    info!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchEval::new(sym_rc);
                builder.eval_arch(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::ODOO);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow().get_tree();
                if already_odoo_rebuilt.contains(&tree) {
                    info!("Already odoo rebuilt, skipping");
                    continue;
                }
                already_odoo_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonOdooBuilder::new(sym_rc);
                builder.load_odoo_content(session);
                continue;
            }
            let sym = session.sync_odoo.pop_item(BuildSteps::VALIDATION);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow_mut().get_tree();
                if already_validation_rebuilt.contains(&tree) {
                    info!("Already validation rebuilt, skipping");
                    continue;
                }
                already_validation_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut validator = PythonValidator::new(sym_rc);
                validator.validate(session);
                if session.sync_odoo.state_init == InitState::ODOO_READY && session.sync_odoo.interrupt_rebuild.load(Ordering::SeqCst) {
                    session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
                    session.log_message(MessageType::INFO, S!("Rebuild interrupted"));
                    session.request_delayed_rebuild();
                    return;
                }
                continue;
            }
        }
        if session.sync_odoo.need_rebuild {
            session.log_message(MessageType::INFO, S!("Rebuild required. Resetting database on breaktime..."));
            SessionInfo::request_reload(session);
        }
    }

    pub fn rebuild_arch_now(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>) {
        session.sync_odoo.rebuild_arch.remove(symbol);
        let mut builder = PythonArchBuilder::new(symbol.clone());
        builder.load_arch(session);
    }

    pub fn add_to_rebuild_arch(&mut self, symbol: Rc<RefCell<Symbol>>) {
        trace!("ADDED TO ARCH - {}", symbol.borrow().paths().first().unwrap_or(symbol.borrow().name()));
        if symbol.borrow().build_status(BuildSteps::ARCH) != BuildStatus::IN_PROGRESS {
            let sym_clone = symbol.clone();
            let mut sym_borrowed = sym_clone.borrow_mut();
            sym_borrowed.set_build_status(BuildSteps::ARCH, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::ODOO, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_arch.insert(symbol);
        }
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: Rc<RefCell<Symbol>>) {
        trace!("ADDED TO EVAL - {}", symbol.borrow().paths().first().unwrap_or(symbol.borrow().name()));
        if symbol.borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS {
            let sym_clone = symbol.clone();
            let mut sym_borrowed = sym_clone.borrow_mut();
            sym_borrowed.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::ODOO, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_arch_eval.insert(symbol);
        }
    }

    pub fn add_to_init_odoo(&mut self, symbol: Rc<RefCell<Symbol>>) {
        trace!("ADDED TO ODOO - {}", symbol.borrow().paths().first().unwrap_or(symbol.borrow().name()));
        if symbol.borrow().build_status(BuildSteps::ODOO) != BuildStatus::IN_PROGRESS {
            let sym_clone = symbol.clone();
            let mut sym_borrowed = sym_clone.borrow_mut();
            sym_borrowed.set_build_status(BuildSteps::ODOO, BuildStatus::PENDING);
            sym_borrowed.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_odoo.insert(symbol);
        }
    }

    pub fn add_to_validations(&mut self, symbol: Rc<RefCell<Symbol>>) {
        trace!("ADDED TO VALIDATION - {}", symbol.borrow().paths().first().unwrap_or(symbol.borrow().name()));
        if symbol.borrow().build_status(BuildSteps::VALIDATION) != BuildStatus::IN_PROGRESS {
            symbol.borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            self.rebuild_validation.insert(symbol);
        }
    }

    pub fn remove_from_rebuild_arch(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch.remove(symbol);
    }

    pub fn remove_from_rebuild_arch_eval(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch_eval.remove(symbol);
    }

    pub fn remove_from_rebuild_odoo(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_odoo.remove(symbol);
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
        if step == BuildSteps::ODOO {
            return self.rebuild_odoo.contains(symbol);
        }
        if step == BuildSteps::VALIDATION {
            return self.rebuild_validation.contains(symbol);
        }
        false
    }

    pub fn get_file_mgr(&mut self) -> Rc<RefCell<FileMgr>> {
        self.file_mgr.clone()
    }

    /* Path must be absolute. Return a valid tree according the root paths and odoo/addons path. The given
    tree may not be in the graph however */
    pub fn tree_from_path(&self, path: &PathBuf) -> Result<Tree, &str> {
        //First check in odoo, before anywhere else
        {
            let odoo_sym = self.symbols.as_ref().unwrap().borrow().get_symbol(&tree(vec!["odoo", "addons"], vec![]), u32::MAX);
            if let Some(odoo_sym) = odoo_sym.get(0).cloned() {
                for addon_path in odoo_sym.borrow().paths().iter() {
                    if path.starts_with(addon_path) {
                        let path = path.strip_prefix(addon_path).unwrap().to_path_buf();
                        let mut tree: Tree = (vec![S!("odoo"), S!("addons")], vec![]);
                        path.components().for_each(|c| {
                            tree.0.push(c.as_os_str().to_str().unwrap().replace(".py", "").replace(".pyi", "").to_string());
                        });
                        if ["__init__", "__manifest__"].contains(&tree.0.last().unwrap().as_str()) {
                            tree.0.pop();
                        }
                        return Ok(tree);
                    }
                }
            }
        }
        for root_path in self.symbols.as_ref().unwrap().borrow().paths().iter() {
            if path.starts_with(root_path) {
                let path = path.strip_prefix(root_path).unwrap().to_path_buf();
                let mut tree: Tree = (vec![], vec![]);
                path.components().for_each(|c| {
                    tree.0.push(c.as_os_str().to_str().unwrap().replace(".py", "").to_string());
                });
                if tree.0.len() > 0 && ["__init__", "__manifest__"].contains(&tree.0.last().unwrap().as_str()) {
                    tree.0.pop();
                }
                return Ok(tree);
            }
        }
        Err("Path not found in any module")
    }

    pub fn _unload_path(session: &mut SessionInfo, path: &PathBuf, clean_cache: bool) -> Result<Rc<RefCell<Symbol>>, String> {
        let ub_symbol = session.sync_odoo.symbols.as_ref().unwrap().clone();
        let symbol = ub_symbol.borrow();
        let path_symbol = symbol.get_symbol(&session.sync_odoo.tree_from_path(&path).unwrap(), u32::MAX);
        if path_symbol.is_empty() {
            return Err("Symbol not found".to_string());
        }
        let path_symbol = path_symbol[0].clone();
        let parent = path_symbol.borrow().parent().clone().unwrap().upgrade().unwrap();
        if clean_cache {
            let file_mgr = session.sync_odoo.file_mgr.clone();
            let mut file_mgr = file_mgr.borrow_mut();
            file_mgr.delete_path(session, &path.sanitize());
            let mut to_del = Vec::from_iter(path_symbol.borrow_mut().all_module_symbol().map(|x| x.clone()));
            let mut index = 0;
            while index < to_del.len() {
                file_mgr.delete_path(session, &to_del[index].borrow().paths()[0]);
                let mut to_del_child = Vec::from_iter(to_del[index].borrow().all_module_symbol().map(|x| x.clone()));
                to_del.append(&mut to_del_child);
                index += 1;
            }
        }
        drop(symbol);
        Symbol::unload(session, path_symbol.clone());
        Ok(parent)
    }

    pub fn create_new_symbol(session: &mut SessionInfo, path: PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<(Rc<RefCell<Symbol>>,Tree)> {
        let mut path = path.clone();
        if path.ends_with("__init__.py") || path.ends_with("__init__.pyi") || path.ends_with("__manifest__.py") {
            path.pop();
        }
        let _arc_symbol = Symbol::create_from_path(session, &path, parent, require_module);
        if _arc_symbol.is_some() {
            let _arc_symbol = _arc_symbol.unwrap();
            session.sync_odoo.add_to_rebuild_arch(_arc_symbol.clone());
            return Some((_arc_symbol.clone(), _arc_symbol.borrow().get_tree().clone()));
        }
        None
    }

    /* Consider the given 'tree' path as updated (or new) and move all symbols that were searching for it
        from the not_found_symbols list to the rebuild list. Return True is something should be rebuilt */
    pub fn search_symbols_to_rebuild(session: &mut SessionInfo, tree: &Tree) -> bool {
        let flat_tree = [tree.0.clone(), tree.1.clone()].concat();
        let mut found_sym: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();
        let mut need_rebuild = false;
        let mut to_add = [vec![], vec![], vec![], vec![]]; //list of symbols to add after the loop (borrow issue)
        for s in session.sync_odoo.not_found_symbols.iter() {
            let mut index: i32 = 0; //i32 sa we could go in negative values
            while (index as usize) < s.borrow().not_found_paths().len() {
                let (step, not_found_tree) = s.borrow().not_found_paths()[index as usize].clone();
                if flat_tree[..cmp::min(not_found_tree.len(), flat_tree.len())] == not_found_tree[..cmp::min(not_found_tree.len(), flat_tree.len())] {
                    need_rebuild = true;
                    match step {
                        BuildSteps::ARCH => {
                            to_add[0].push(s.clone());
                        },
                        BuildSteps::ARCH_EVAL => {
                            to_add[1].push(s.clone());
                        },
                        BuildSteps::ODOO => {
                            to_add[2].push(s.clone());
                        },
                        BuildSteps::VALIDATION => {
                            to_add[3].push(s.clone());
                        },
                        _ => {}
                    }
                    s.borrow_mut().not_found_paths_mut().remove(index as usize);
                    index -= 1;
                }
                index += 1;
            }
            if s.borrow().not_found_paths().len() == 0 {
                found_sym.insert(s.clone());
            }
        }
        for s in to_add[0].iter() {
            session.sync_odoo.add_to_rebuild_arch(s.clone());
        }
        for s in to_add[1].iter() {
            session.sync_odoo.add_to_rebuild_arch_eval(s.clone());
        }
        for s in to_add[2].iter() {
            session.sync_odoo.add_to_init_odoo(s.clone());
        }
        for s in to_add[3].iter() {
            s.borrow_mut().invalidate_sub_functions(session);
            session.sync_odoo.add_to_validations(s.clone());
        }
        for sym in found_sym.iter() {
            session.sync_odoo.not_found_symbols.remove(&sym);
        }
        need_rebuild
    }

    pub fn get_file_symbol(&self, path: &PathBuf) -> Option<Rc<RefCell<Symbol>>> {
        let symbol = self.symbols.as_ref().unwrap().borrow();
        let tree = &self.tree_from_path(&path);
        if let Ok(tree) = tree {
            return symbol.get_symbol(tree, u32::MAX).get(0).cloned();
        } else {
            error!("Path {} not found", path.to_str().expect("unable to stringify path"));
            None
        }
    }

    pub fn refresh_evaluations(session: &mut SessionInfo) {
        let mut symbols = vec![session.sync_odoo.symbols.clone().unwrap()];
        while symbols.len() > 0 {
            let s = symbols.pop();
            if let Some(s) = s {
                if s.borrow().in_workspace() && matches!(&s.borrow().typ(), SymType::FILE | SymType::PACKAGE(_)) {
                    session.sync_odoo.add_to_rebuild_arch_eval(s.clone());
                }
                symbols.extend(s.borrow().all_module_symbol().map(|x| {x.clone()}) );
            }
        }
        SyncOdoo::process_rebuilds(session);
    }

    pub fn get_rebuild_queue_size(&self) -> usize {
        return self.rebuild_arch.len() + self.rebuild_arch_eval.len() + self.rebuild_odoo.len() + self.rebuild_validation.len()
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
        let config = session.send_request::<ConfigurationParams, Vec<serde_json::Value>>(WorkspaceConfiguration::METHOD, config_params).unwrap().unwrap();
        let python_path = session.send_request::<(), PythonPathRequestResult>(PythonPathRequest::METHOD, ());
        if let Err(_e) = python_path {
            session.log_message(MessageType::ERROR, S!("Unable to get PythonPath. Be sure that your editor support the route Odoo/getPythonPath"));
            return Err(format!("{:?}", _e));
        }
        let python_path = python_path.unwrap();
        let python_path = match python_path {
            Some(p) => {p.python_path},
            None => {
                session.log_message(MessageType::WARNING, S!("No PythonPath provided. Be sure that your editor support the route Odoo/getPythonPath and that route always return a result. Using 'python3' instead"));
                S!("python3")
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
                            _auto_save_delay = refresh_delay;
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
                    "serverLogLevel" => {
                        //Too late, set it with command line
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
            config.odoo_path = odoo_conf.get("odooPath").expect("odooPath must exist").as_str().expect("odooPath must be a String").to_string();
        } else {
            config.addons = vec![];
            config.odoo_path = S!("");
            session.log_message(MessageType::ERROR, S!("Unable to find selected configuration. No odoo path has been found."));
        }
        config.python_path = python_path.clone();
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
        if params.text_document_position_params.text_document.uri.to_string().ends_with(".py") {
            if let Some(file_symbol) = session.sync_odoo.get_file_symbol(&PathBuf::from(path.clone())) {
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
        if params.text_document_position_params.text_document.uri.to_string().ends_with(".py") {
            if let Some(file_symbol) = session.sync_odoo.get_file_symbol(&PathBuf::from(path.clone())) {
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
            if let Some(file_symbol) = session.sync_odoo.get_file_symbol(&PathBuf::from(path.clone())) {
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
                    config.stdlib != old_config.stdlib {
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
            if Odoo::update_file_cache(session, path.clone(), None, -100) {
                Odoo::update_file_index(session, path, true, false);
            }
        }
    }

    pub fn handle_did_open(session: &mut SessionInfo, params: DidOpenTextDocumentParams) {
        //to implement Incremental update of file caches, we have to handle DidOpen notification, to be sure
        // that we use the same base version of the file for future incrementation.
        let path = params.text_document.uri.to_file_path().unwrap();
        session.log_message(MessageType::INFO, format!("File opened: {}", path.sanitize()));
        if Odoo::update_file_cache(session, path.clone(), Some(&vec![TextDocumentContentChangeEvent{
            range: None,
            range_length: None,
                text: params.text_document.text}]), params.text_document.version) {
            if session.sync_odoo.config.refresh_mode == RefreshMode::Off || session.sync_odoo.state_init == InitState::NOT_READY {
                return
            }
            Odoo::update_file_index(session, path,true, true);
        }
    }

    pub fn handle_did_close(session: &mut SessionInfo, params: DidCloseTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();
        session.log_message(MessageType::INFO, format!("File closed: {}", path.sanitize()));
        let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path.to_str().unwrap().to_string());
        if let Some(file_info) = file_info {
            file_info.borrow_mut().opened = false;
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
            let _ = SyncOdoo::_unload_path(session, &PathBuf::from(&old_path), false);
            session.sync_odoo.get_file_mgr().borrow_mut().delete_path(session, &old_path);
            //2 - create new document
            let tree = session.sync_odoo.tree_from_path(&PathBuf::from(new_path));
            if let Ok(tree) = tree {
                SyncOdoo::search_symbols_to_rebuild(session, &tree);
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
            session.log_message(MessageType::INFO, format!("Creating {}", path));
            //1 - delete old uri
            let tree = session.sync_odoo.tree_from_path(&PathBuf::from(path));
            if let Ok(tree) = tree {
                SyncOdoo::search_symbols_to_rebuild(session, &tree);
            }
        }
        SyncOdoo::process_rebuilds(session);
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
            session.sync_odoo.get_file_mgr().borrow_mut().delete_path(session, &path);
        }
        SyncOdoo::process_rebuilds(session);
    }

    pub fn handle_did_change(session: &mut SessionInfo, params: DidChangeTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();
        session.log_message(MessageType::INFO, format!("File changed: {}", path.sanitize()));
        let version = params.text_document.version;
        if Odoo::update_file_cache(session, path.clone(), Some(&params.content_changes), version) {
            if (session.sync_odoo.config.refresh_mode != RefreshMode::AfterDelay && session.sync_odoo.config.refresh_mode != RefreshMode::Adaptive) || session.sync_odoo.state_init == InitState::NOT_READY {
                return
            }
            Odoo::update_file_index(session, path, false, false);
        }
    }

    pub fn handle_did_save(session: &mut SessionInfo, params: DidSaveTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();
        session.log_message(MessageType::INFO, format!("File saved: {}", path.sanitize()));
        if session.sync_odoo.config.refresh_mode != RefreshMode::OnSave || session.sync_odoo.state_init == InitState::NOT_READY {
            return
        }
        Odoo::update_file_index(session, path,true, false);
    }

    // return true if the file has been updated, is valid for an index reload, and contents have been changed
    fn update_file_cache(session: &mut SessionInfo, path: PathBuf, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: i32) -> bool {
        if path.extension().is_some() && path.extension().unwrap() == "py" {
            let tree = session.sync_odoo.tree_from_path(&path);
            if let Err(_e) = tree { //is not part of odoo (or not in addons path)
                return false;
            }
            session.log_message(MessageType::INFO, format!("File Change Event: {}, version {}", path.to_str().unwrap(), version));
            let (file_updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &path.sanitize(), content, Some(version), false);
            file_info.borrow_mut().publish_diagnostics(session); //To push potential syntax errors or refresh previous one
            return file_info.borrow().valid && (!file_info.borrow().opened || version >= 0) && file_updated;
        }
        false
    }

    pub fn update_file_index(session: &mut SessionInfo, path: PathBuf, is_save: bool, is_open: bool) {
        if path.extension().is_some() && path.extension().unwrap() == "py" {
            if is_open || (is_save && session.sync_odoo.config.refresh_mode == RefreshMode::OnSave) {
                let tree = session.sync_odoo.tree_from_path(&path);
                if !tree.is_err() { //is part of odoo (and in addons path)
                    let tree = tree.unwrap().clone();
                    let _ = SyncOdoo::_unload_path(session, &path, false);
                    SyncOdoo::search_symbols_to_rebuild(session, &tree);
                }
                SyncOdoo::process_rebuilds(session);
            } else {
                if session.sync_odoo.config.refresh_mode == RefreshMode::AfterDelay || session.sync_odoo.config.refresh_mode == RefreshMode::Adaptive {
                    SessionInfo::request_update_file_index(session, &path);
                }
            }
        }
    }

}
