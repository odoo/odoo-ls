use crate::core::config::{Config, ConfigRequest};
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;

use std::collections::HashSet;
use weak_table::PtrWeakHashSet;
use std::process::Command;
use std::str::FromStr;
use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use crate::constants::*;
use super::config::{self, DiagMissingImportsMode, RefreshMode};
use super::file_mgr::FileMgr;
use super::symbol::Symbol;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::core::python_arch_eval::PythonArchEval;
use crate::core::messages::Msg;
//use super::python_arch_builder::PythonArchBuilder;

#[derive(Debug)]
pub struct SyncOdoo {
    pub version_major: u32,
    pub version_minor: u32,
    pub version_micro: u32,
    pub full_version: String,
    pub config: Config,
    pub symbols: Option<Rc<RefCell<Symbol>>>,
    pub builtins: Option<Rc<RefCell<Symbol>>>,
    pub stubs_dir: String,
    pub stdlib_dir: String,
    file_mgr: Rc<RefCell<FileMgr>>,
    pub modules: HashMap<String, Weak<RefCell<Symbol>>>,
    rebuild_arch: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_arch_eval: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_odoo: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_validation: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub not_found_symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub msg_sender: tokio::sync::mpsc::Sender<Msg>,
}

unsafe impl Send for SyncOdoo {}

impl SyncOdoo {

    pub fn new(msg_sender: tokio::sync::mpsc::Sender<Msg>) -> Self {
        let symbols = Rc::new(RefCell::new(Symbol::new_root("root".to_string(), SymType::ROOT)));
        let builtins = Rc::new(RefCell::new(Symbol::new_root("builtins".to_string(), SymType::ROOT)));
        builtins.borrow_mut().weak_self = Some(Rc::downgrade(&builtins)); // manually set weakself for root symbols
        symbols.borrow_mut().weak_self = Some(Rc::downgrade(&symbols)); // manually set weakself for root symbols
        let sync_odoo = Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            config: Config::new(),
            symbols: Some(symbols),
            builtins: Some(builtins),
            file_mgr: Rc::new(RefCell::new(FileMgr::new())),
            stubs_dir: PathBuf::from("./../server/typeshed/stubs").to_str().unwrap().to_string(),
            stdlib_dir: PathBuf::from("./../server/typeshed/stdlib").to_str().unwrap().to_string(),
            modules: HashMap::new(),
            rebuild_arch: PtrWeakHashSet::new(),
            rebuild_arch_eval: PtrWeakHashSet::new(),
            rebuild_odoo: PtrWeakHashSet::new(),
            rebuild_validation: PtrWeakHashSet::new(),
            not_found_symbols: PtrWeakHashSet::new(),
            msg_sender,
        };
        sync_odoo
    }

    pub fn get_symbol(&self, tree: &Tree) -> Option<Rc<RefCell<Symbol>>> {
        self.symbols.as_ref().unwrap().borrow_mut().get_symbol(&tree)
    }

    fn pop_item(&mut self, step: BuildSteps) -> Option<Rc<RefCell<Symbol>>> {
        let mut arc_sym: Option<Rc<RefCell<Symbol>>> = None;
        //Part 1: Find the symbol with a unmutable set
        {
            let set =  if step == BuildSteps::ARCH_EVAL {
                &self.rebuild_arch_eval
            } else if step == BuildSteps::ODOO {
                &self.rebuild_odoo
            } else if step == BuildSteps::VALIDATION {
                &self.rebuild_validation
            } else {
                &self.rebuild_arch
            };
            let mut selected_sym: Option<Rc<RefCell<Symbol>>> = None;
            let mut selected_count: u32 = 999999999;
            let mut current_count: u32;
            for sym in &*set {
                current_count = 0;
                let symbol = sym.borrow_mut();
                for (index, dep_set) in symbol.get_all_dependencies(step).iter().enumerate() {
                    if index == BuildSteps::ARCH as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_arch.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::ARCH_EVAL as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_arch_eval.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::ODOO as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_odoo.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::VALIDATION as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_validation.contains(&dep) {
                                current_count += 1;
                            }
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
            let set =  if step == BuildSteps::ARCH_EVAL {
                &mut self.rebuild_arch_eval
            } else if step == BuildSteps::ODOO {
                &mut self.rebuild_odoo
            } else if step == BuildSteps::VALIDATION {
                &mut self.rebuild_validation
            } else {
                &mut self.rebuild_arch
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

    fn process_rebuilds(&mut self) {
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::new();
        while !self.rebuild_arch.is_empty() || !self.rebuild_arch_eval.is_empty() {
            let sym = self.pop_item(BuildSteps::ARCH);
            if sym.is_some() {
                let sym_arc = sym.as_ref().unwrap().clone();
                let tree = sym_arc.borrow_mut().get_tree().clone();
                if already_arch_rebuilt.contains(&tree) {
                    println!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchBuilder::new(sym_arc);
                builder.load_arch(self);
                continue;
            }
            let sym = self.pop_item(BuildSteps::ARCH_EVAL);
            if sym.is_some() {
                let sym_arc = sym.as_ref().unwrap().clone();
                let tree = sym_arc.borrow_mut().get_tree().clone();
                if already_arch_eval_rebuilt.contains(&tree) {
                    println!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchEval::new(sym_arc);
                builder.eval_arch(self);
                continue;
            }
        }
    }

    pub fn add_to_rebuild_arch(&mut self, symbol: Rc<RefCell<Symbol>>) {
        self.rebuild_arch.insert(symbol);
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: Rc<RefCell<Symbol>>) {
        self.rebuild_arch_eval.insert(symbol);
    }

    pub fn add_to_init_odoo(&mut self, symbol: Rc<RefCell<Symbol>>) {
        self.rebuild_odoo.insert(symbol);
    }

    pub fn add_to_validations(&mut self, symbol: Rc<RefCell<Symbol>>) {
        self.rebuild_validation.insert(symbol);
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

    /* Path must be absolute. */
    pub fn tree_from_path(&self, path: &PathBuf) -> Result<Tree, &str> {
        for root_sym in self.symbols.as_ref().unwrap().borrow().module_symbols.values() {
            let root_path = root_sym.borrow().paths.clone();
            for rp in root_path.iter() {
                if path.starts_with(rp) {
                    let path = path.strip_prefix(rp).unwrap().to_path_buf();
                    let mut tree: Tree = (vec![], vec![]);
                    path.components().for_each(|c| {
                        tree.0.push(c.as_os_str().to_str().unwrap().to_string());
                    });
                    return Ok(tree);
                }
            }
        }
        Err("Path not found in any module")
    }

    pub fn _unload_path(&mut self, path: PathBuf, clean_cache: bool) -> Result<Rc<RefCell<Symbol>>, &str> {
        let symbol = self.symbols.as_ref().unwrap().borrow();
        let path_symbol = symbol.get_symbol(&self.tree_from_path(&path).unwrap());
        if path_symbol.is_none() {
            return Err("Symbol not found");
        }
        let path_symbol = path_symbol.unwrap();
        let parent = path_symbol.borrow().parent.clone().unwrap().upgrade().unwrap();
        let mut parent_mut = parent.borrow_mut();
        if clean_cache {
            let mut file_mgr = self.file_mgr.borrow_mut();
            file_mgr.delete_path(self, path.as_os_str().to_str().unwrap().to_string());
            let mut to_del = Vec::from_iter(path_symbol.borrow_mut().module_symbols.values().map(|x| x.clone()));
            let mut index = 0;
            while index < to_del.len() {
                file_mgr.delete_path(self, to_del[index].borrow().paths[0].clone());
                let mut to_del_child = Vec::from_iter(to_del[index].borrow().module_symbols.values().map(|x| x.clone()));
                to_del.append(&mut to_del_child);
                index += 1;
            }
        }
        drop(symbol);
        Symbol::unload(self, path_symbol);
        Ok(parent.clone())
    }
}

#[derive(Debug)]
pub struct Odoo {
    pub odoo: Arc<Mutex<SyncOdoo>>,
    pub msg_sender: tokio::sync::mpsc::Sender<Msg>,
}

impl Odoo {
    pub fn new(sx: tokio::sync::mpsc::Sender<Msg>) -> Self {
        Self {
            odoo: Arc::new(Mutex::new(SyncOdoo::new(sx.clone()))),
            msg_sender: sx.clone(),
        }
    }

    pub async fn init(&mut self, client: &Client) {
        self.msg_sender.send(Msg::LOG_INFO(String::from("Building new Odoo knowledge database"))).await.expect("Unable to send message");
        let response = client.send_request::<ConfigRequest>(()).await.unwrap();
        let _odoo = self.odoo.clone();
        let configuration_item = ConfigurationItem{
            scope_uri: None,
            section: Some("Odoo".to_string()),
        };
        let config = client.configuration(vec![configuration_item]).await.unwrap();
        let config = config.get(0);
        if !config.is_some() {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("No config found for Odoo. Exiting..."))).await.expect("Unable to send message");
            std::process::exit(1);
        }
        let config = config.unwrap();
        //values for sync block
        let mut _refresh_mode : RefreshMode = RefreshMode::OnSave;
        let mut _auto_save_delay : u64 = 2000;
        let mut _diag_missing_imports : DiagMissingImportsMode = DiagMissingImportsMode::All;
        if let Some(map) = config.as_object() {
            for (key, value) in map {
                match key.as_str() {
                    "autoRefresh" => {
                        if let Some(refresh_mode) = value.as_str() {
                            _refresh_mode = match RefreshMode::from_str(refresh_mode) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to parse RefreshMode. Setting it to onSave"))).await.expect("Unable to send message");
                                    RefreshMode::OnSave
                                }
                            };
                        }
                    },
                    "autoRefreshDelay" => {
                        if let Some(refresh_delay) = value.as_u64() {
                            _auto_save_delay = refresh_delay;
                        } else {
                            self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to parse auto_save_delay. Setting it to 2000"))).await.expect("Unable to send message");
                            _auto_save_delay = 2000
                        }
                    },
                    "diagMissingImportLevel" => {
                        if let Some(diag_import_level) = value.as_str() {
                            _diag_missing_imports = match DiagMissingImportsMode::from_str(diag_import_level) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to parse diag_import_level. Setting it to all"))).await.expect("Unable to send message");
                                    DiagMissingImportsMode::All
                                }
                            };
                        }
                    },
                    _ => {
                        self.msg_sender.send(Msg::LOG_ERROR(format!("Unknown config key: {}", key))).await.expect("Unable to send message");
                    },
                }
            }
        }
        tokio::task::spawn_blocking(move || {
            let mut sync_odoo = _odoo.lock().unwrap();
            sync_odoo.config.addons = response.addons.clone();
            sync_odoo.config.odoo_path = response.odoo_path.clone();
            sync_odoo.config.python_path = response.python_path.clone();
            sync_odoo.config.refresh_mode = _refresh_mode;
            sync_odoo.config.auto_save_delay = _auto_save_delay;
            sync_odoo.config.diag_missing_imports = _diag_missing_imports;
            {
                let mut root_symbol = sync_odoo.symbols.as_ref().unwrap().borrow_mut();
                root_symbol.paths.push(sync_odoo.stdlib_dir.clone());
                root_symbol.paths.push(sync_odoo.stubs_dir.clone());
                //TODO add sys.path
                let output = Command::new(sync_odoo.config.python_path.clone()).args(&["-c", "import sys; print(sys.path)"]).output().expect("Can't exec python3");
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    sync_odoo.msg_sender.blocking_send(Msg::LOG_INFO(format!("Detected sys.path: {}", stdout))).expect("Unable to send message");
                    // extract vec of string from output
                    if stdout.len() > 5 {
                        let values = String::from((stdout[2..stdout.len()-3]).to_string());
                        for value in values.split("', '") {
                            let value = value.to_string();
                            if value.len() > 0 {
                                let pathbuf = PathBuf::from(value.clone());
                                if pathbuf.is_dir() {
                                    sync_odoo.msg_sender.blocking_send(Msg::LOG_INFO(format!("Adding sys.path: {}", stdout))).expect("Unable to send message");
                                    root_symbol.paths.push(value.clone());
                                }
                            }
                        }
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("{}", stderr);
                }
            }
        }).await.unwrap();
        self.load_builtins(&client).await;
        self.build_database(&client).await;
    }

    async fn load_builtins(&mut self, client: &Client) {
        let path = PathBuf::from("./../server/typeshed/stdlib/builtins.pyi");
        let builtins_path = fs::canonicalize(path);
        let Ok(builtins_path) = builtins_path else {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to find builtins.pyi"))).await.expect("Unable to send message");
            return;
        };
        let _odoo = self.odoo.clone();
        tokio::task::spawn_blocking(move || {
            let mut sync_odoo = _odoo.lock().unwrap();
            if sync_odoo.builtins.is_none() {
                panic!("Builtins symbol not found")
            }
            let builtins = sync_odoo.builtins.as_ref().unwrap().clone();
            let _builtins_arc_symbol = Symbol::create_from_path(&mut sync_odoo, &builtins_path, builtins, false);
            sync_odoo.add_to_rebuild_arch(_builtins_arc_symbol.unwrap());
            sync_odoo.process_rebuilds();
        }).await.unwrap();
    }

    async fn build_database(&mut self, client: &Client) {
        self.msg_sender.send(Msg::LOG_INFO(String::from("Building Database"))).await.expect("Unable to send message");
        let result = self.build_base(client).await;
        if result {
            self.build_modules(client).await;
        }
    }

    async fn build_base(&mut self, client: &Client) -> bool {
        let odoo_path = self.odoo.lock().unwrap().config.odoo_path.clone();
        let release_path = PathBuf::from(odoo_path.clone()).join("odoo/release.py");
        let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
        if !release_path.exists() {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to find release.py - Aborting"))).await.expect("Unable to send message");
            return false;
        }
        // open release.py and get version
        let release_file = fs::read_to_string(release_path);
        let release_file = match release_file {
            Ok(release_file) => release_file,
            Err(_) => {
                self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to read release.py - Aborting"))).await.expect("Unable to send message");
                return false;
            }
        };
        let mut _version_major: u32 = 14;
        let mut _version_minor: u32 = 0;
        let mut _version_micro: u32 = 0;
        let mut _full_version: String = "14.0.0".to_string();
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
                        _full_version = format!("{}.{}.{}", _version_major, _version_minor, _version_micro);
                        break;
                    },
                    None => {
                        self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to detect the Odoo version. Running the tool for the version 14"))).await.expect("Unable to send message");
                        return false;
                    }
                }
            }
        }
        self.msg_sender.send(Msg::LOG_INFO(format!("Odoo version: {}", _full_version))).await.expect("Unable to send message");
        if _version_major < 14 {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("Odoo version is less than 14. The tool only supports version 14 and above. Aborting..."))).await.expect("Unable to send message");
            return false;
        }
        let _odoo = self.odoo.clone();
        let res = tokio::task::spawn_blocking(move || {
            let mut sync_odoo = _odoo.lock().unwrap();
            sync_odoo.version_major = _version_major;
            sync_odoo.version_minor = _version_minor;
            sync_odoo.version_micro = _version_micro;
            sync_odoo.full_version = _full_version;
            //build base
            sync_odoo.symbols.as_ref().unwrap().borrow_mut().paths.push(sync_odoo.config.odoo_path.clone());
            if sync_odoo.symbols.is_none() {
                panic!("Odoo root symbol not found")
            }
            let root_symbol = sync_odoo.symbols.as_ref().unwrap().clone();
            let config_odoo_path = sync_odoo.config.odoo_path.clone();
            let added_symbol = Symbol::create_from_path(&mut sync_odoo, &PathBuf::from(config_odoo_path).join("odoo"),  root_symbol, false);
            sync_odoo.add_to_rebuild_arch(added_symbol.unwrap());
            sync_odoo.process_rebuilds();
            //search common odoo addons path
            let addon_symbol = sync_odoo.get_symbol(&tree(vec!["odoo", "addons"], vec![]));
            if odoo_addon_path.exists() {
                addon_symbol.as_ref().unwrap().borrow_mut().paths.push(
                    odoo_addon_path.to_str().unwrap().to_string()
                );
            } else {
                return false;
            }
            return true
        }).await.unwrap();
        if !res {
            let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
            self.msg_sender.send(Msg::LOG_ERROR(format!("Unable to find odoo addons path at {}", odoo_addon_path.to_str().unwrap().to_string()))).await.expect("Unable to send message");
        }
        return res;
    }

    async fn build_modules(&mut self, client: &Client) {
        let _odoo = self.odoo.clone();
        let res = tokio::task::spawn_blocking(move || {
            let mut sync_odoo = _odoo.lock().unwrap();
            {
                let addons_symbol = sync_odoo.get_symbol(&tree(vec!["odoo", "addons"], vec![])).expect("Unable to find odoo addons symbol");
                let addons_path = addons_symbol.borrow_mut().paths.clone();
                for addon_path in addons_path.iter() {
                    if PathBuf::from(addon_path).exists() {
                        //browse all dir in path
                        for item in PathBuf::from(addon_path).read_dir().expect("Unable to find odoo addons path") {
                            match item {
                                Ok(item) => {
                                    if item.file_type().unwrap().is_dir() {
                                        let module_symbol = Symbol::create_from_path(&mut sync_odoo, &item.path(), addons_symbol.clone(), true);
                                        if module_symbol.is_some() {
                                            sync_odoo.add_to_rebuild_arch(module_symbol.unwrap());
                                        }
                                    }
                                },
                                Err(_) => {}
                            }
                        }
                    }
                }
            }
            sync_odoo.process_rebuilds();
            println!("{}", sync_odoo.symbols.as_ref().unwrap().borrow_mut().debug_print_graph());
        }).await.unwrap();
        self.msg_sender.send(Msg::LOG_INFO(String::from("End building modules."))).await.expect("Unable to send message");
    }

    pub async fn reload_file(&mut self, client: &Client, path: PathBuf, content: String, version: i32) {
        if path.extension().is_some() && path.extension().unwrap() == "py" {
            client.log_message(MessageType::INFO, format!("File Change Event: {}, version {}", path.to_str().unwrap(), version)).await;
            let _odoo = self.odoo.clone();
            tokio::task::spawn_blocking(move || {
                let mut sync_odoo = _odoo.lock().unwrap();
                let odoo = &mut sync_odoo;
                let file_info = odoo.get_file_mgr().borrow_mut().get_file_info(odoo, &path.as_os_str().to_str().unwrap().to_string(), Some(content), Some(version));
                let mut mut_file_info = file_info.borrow_mut();
                mut_file_info.publish_diagnostics(odoo); //To push potential syntax errors or refresh previous one
                let parent = odoo._unload_path(path, false);
                if parent.is_err() {
                    return;
                }
                //build new
                //search for missing symbols
            }).await.expect("An error occured while executing reload_file sync block");
        }
    }

}