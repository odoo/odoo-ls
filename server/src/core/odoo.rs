use crate::core::config::{Config, ConfigRequest};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;

use std::collections::HashSet;
use std::process::Command;
use std::str::FromStr;
use std::fs;
use std::path::PathBuf;
use regex::Regex;
use crate::constants::*;
use super::config::{self, DiagMissingImportsMode, RefreshMode};
use super::symbol::Symbol;
use crate::my_weak::MyWeak;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::core::python_arch_eval::PythonArchEval;
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
    rebuild_arch: HashSet<MyWeak<RefCell<Symbol>>>,
    rebuild_arch_eval: HashSet<MyWeak<RefCell<Symbol>>>,
    rebuild_odoo: HashSet<MyWeak<RefCell<Symbol>>>,
    rebuild_validation: HashSet<MyWeak<RefCell<Symbol>>>,
    pub not_found_symbols: HashSet<MyWeak<RefCell<Symbol>>>,
}

unsafe impl Send for SyncOdoo {}

impl SyncOdoo {

    pub fn new() -> Self {
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
            stubs_dir: PathBuf::from("./../server/typeshed/stubs").to_str().unwrap().to_string(),
            stdlib_dir: PathBuf::from("./../server/typeshed/stdlib").to_str().unwrap().to_string(),
            rebuild_arch: HashSet::new(),
            rebuild_arch_eval: HashSet::new(),
            rebuild_odoo: HashSet::new(),
            rebuild_validation: HashSet::new(),
            not_found_symbols: HashSet::new(),
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
            let mut selected_sym: Option<&MyWeak<RefCell<Symbol>>> = None;
            let mut selected_count: u32 = 999999999;
            let mut current_count: u32;
            for sym in &*set {
                current_count = 0;
                let mut_symbol = sym.upgrade();
                if mut_symbol.is_none() {
                    //println!("missing symbol ! ");
                    continue;
                }
                let mut_symbol = mut_symbol.unwrap();
                let symbol = mut_symbol.borrow_mut();
                for (index, dep_set) in symbol.get_all_dependencies(step).iter().enumerate() {
                    if index == BuildSteps::ARCH as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_arch.contains(dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::ARCH_EVAL as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_arch_eval.contains(dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::ODOO as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_odoo.contains(dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::VALIDATION as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_validation.contains(dep) {
                                current_count += 1;
                            }
                        }
                    }
                }
                if current_count < selected_count {
                    selected_sym = Some(&sym);
                    selected_count = current_count;
                    if current_count == 0 {
                        break;
                    }
                }
            }
            if selected_sym.is_some() {
                arc_sym = selected_sym.unwrap().upgrade()
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
            if !set.remove(&MyWeak::new(Rc::downgrade(&arc_sym_unwrapped))) {
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

    pub fn add_to_rebuild_arch(&mut self, symbol: Weak<RefCell<Symbol>>) {
        self.rebuild_arch.insert(MyWeak::new(symbol));
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: Weak<RefCell<Symbol>>) {
        self.rebuild_arch_eval.insert(MyWeak::new(symbol));
    }

    pub fn is_in_rebuild(&self, symbol: &Weak<RefCell<Symbol>>, step: BuildSteps) -> bool {
        if step == BuildSteps::ARCH {
            return self.rebuild_arch.contains(&MyWeak::new(symbol.clone()));
        }
        if step == BuildSteps::ARCH_EVAL {
            return self.rebuild_arch_eval.contains(&MyWeak::new(symbol.clone()));
        }
        if step == BuildSteps::ODOO {
            return self.rebuild_odoo.contains(&MyWeak::new(symbol.clone()));
        }
        if step == BuildSteps::VALIDATION {
            return self.rebuild_validation.contains(&MyWeak::new(symbol.clone()));
        }
        false
    }
}

#[derive(Debug)]
pub struct Odoo {
    pub odoo: Arc<Mutex<SyncOdoo>>,
}

impl Odoo {
    pub fn new() -> Self {
        Self {
            odoo: Arc::new(Mutex::new(SyncOdoo::new()))
        }
    }

    pub async fn init(&mut self, client: &Client) {
        client.log_message(MessageType::INFO, "Building new Odoo knowledge database").await;
        let response = client.send_request::<ConfigRequest>(()).await.unwrap();
        let _odoo = self.odoo.clone();
        let configuration_item = ConfigurationItem{
            scope_uri: None,
            section: Some("Odoo".to_string()),
        };
        let config = client.configuration(vec![configuration_item]).await.unwrap();
        let config = config.get(0);
        if !config.is_some() {
            client.log_message(MessageType::ERROR, "No config found for Odoo. Exiting...").await;
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
                                    client.log_message(MessageType::ERROR, "Unable to parse RefreshMode. Setting it to onSave").await;
                                    RefreshMode::OnSave
                                }
                            };
                        }
                    },
                    "autoRefreshDelay" => {
                        if let Some(refresh_delay) = value.as_u64() {
                            _auto_save_delay = refresh_delay;
                        } else {
                            client.log_message(MessageType::ERROR, "Unable to parse auto_save_delay. Setting it to 2000").await;
                            _auto_save_delay = 2000
                        }
                    },
                    "diagMissingImportLevel" => {
                        if let Some(diag_import_level) = value.as_str() {
                            _diag_missing_imports = match DiagMissingImportsMode::from_str(diag_import_level) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    client.log_message(MessageType::ERROR, "Unable to parse diag_import_level. Setting it to all").await;
                                    DiagMissingImportsMode::All
                                }
                            };
                        }
                    },
                    _ => {
                        client.log_message(MessageType::ERROR, format!("Unknown config key: {}", key)).await;
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
                    println!("Detected sys.path: {}", stdout);
                    // extract vec of string from output
                    if stdout.len() > 5 {
                        let values = String::from((stdout[2..stdout.len()-3]).to_string());
                        for value in values.split("', '") {
                            let value = value.to_string();
                            if value.len() > 0 {
                                let pathbuf = PathBuf::from(value.clone());
                                if pathbuf.is_dir() {
                                    println!("Adding sys.path: {}", value);
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
            client.log_message(MessageType::ERROR, "Unable to find builtins.pyi").await;
            return;
        };
        let _odoo = self.odoo.clone();
        tokio::task::spawn_blocking(move || {
            let mut sync_odoo = _odoo.lock().unwrap();
            let _builtins_arc_symbol = match sync_odoo.builtins {
                Some(ref builtins) => {
                    let mut b = builtins.borrow_mut();
                    let _builtins_symbol = Symbol::create_from_path(&builtins_path, &b, false);
                    b.add_symbol(&sync_odoo, _builtins_symbol.unwrap())
                },
                None => panic!("Builtins symbol not found")
            };
            sync_odoo.add_to_rebuild_arch(Rc::downgrade(&_builtins_arc_symbol));
            sync_odoo.process_rebuilds();
        }).await.unwrap();
    }

    async fn build_database(&mut self, client: &Client) {
        client.log_message(MessageType::INFO, "Building Database").await;
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
            client.log_message(MessageType::ERROR, "Unable to find release.py - Aborting").await;
            return false;
        }
        // open release.py and get version
        let release_file = fs::read_to_string(release_path);
        let release_file = match release_file {
            Ok(release_file) => release_file,
            Err(_) => {
                client.log_message(MessageType::ERROR, "Unable to read release.py - Aborting").await;
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
                        client.log_message(MessageType::ERROR, "Unable to detect the Odoo version. Running the tool for the version 14").await;
                        return false;
                    }
                }
            }
        }
        client.log_message(MessageType::INFO, format!("Odoo version: {}", _full_version)).await;
        if _version_major < 14 {
            client.log_message(MessageType::ERROR, "Odoo version is less than 14. The tool only supports version 14 and above. Aborting...").await;
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
            let _odoo_arc_symbol = match sync_odoo.symbols {
                Some(ref symbols) => {
                    let mut s = symbols.borrow_mut();
                    let _odoo_symbol = Symbol::create_from_path(&PathBuf::from(sync_odoo.config.odoo_path.clone()).join("odoo"), &s, false);
                    s.add_symbol(&sync_odoo, _odoo_symbol.unwrap())
                },
                None => panic!("Odoo root symbol not found")
            };
            sync_odoo.add_to_rebuild_arch(Rc::downgrade(&_odoo_arc_symbol));
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
            client.log_message(MessageType::ERROR, format!("Unable to find odoo addons path at {}", odoo_addon_path.to_str().unwrap().to_string())).await;
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
                                        let mut a_m = addons_symbol.borrow_mut();
                                        let module_symbol = Symbol::create_from_path(&item.path(), &a_m, true);
                                        if module_symbol.is_some() {
                                            let _odoo_arc_symbol = a_m.add_symbol(&sync_odoo, module_symbol.unwrap());
                                            sync_odoo.add_to_rebuild_arch(Rc::downgrade(&_odoo_arc_symbol));
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
        client.log_message(MessageType::INFO, "End building modules.").await;
    }

}