use crate::core::config::{Config, ConfigRequest};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;

use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Weak, Mutex};
use std::str::FromStr;
use std::fs;
use std::path::PathBuf;
use regex::Regex;
use crate::constants::*;
use super::config::{RefreshMode, DiagMissingImportsMode};
use super::symbol::Symbol;
use crate::my_weak::MyWeak;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::core::python_arch_eval::PythonArchEval;
use crate::utils::is_dir_cs;
//use super::python_arch_builder::PythonArchBuilder;

#[derive(Debug)]
pub struct Odoo {
    pub version_major: u32,
    pub version_minor: u32,
    pub version_micro: u32,
    pub full_version: String,
    pub config: Config,
    pub symbols: Option<Arc<Mutex<Symbol>>>,
    pub builtins: Option<Arc<Mutex<Symbol>>>,
    pub stubs_dir: String,
    pub stdlib_dir: String,
    rebuild_arch: HashSet<MyWeak<Mutex<Symbol>>>,
    rebuild_arch_eval: HashSet<MyWeak<Mutex<Symbol>>>,
    rebuild_odoo: HashSet<MyWeak<Mutex<Symbol>>>,
    rebuild_validation: HashSet<MyWeak<Mutex<Symbol>>>,
}

impl Odoo {
    pub fn new() -> Self {
        let symbols = Arc::new(Mutex::new(Symbol::new_root("root".to_string(), SymType::ROOT)));
        let builtins = Arc::new(Mutex::new(Symbol::new_root("builtins".to_string(), SymType::ROOT)));
        builtins.lock().unwrap().weak_self = Some(Arc::downgrade(&builtins)); // manually set weakself for root symbols
        symbols.lock().unwrap().weak_self = Some(Arc::downgrade(&symbols)); // manually set weakself for root symbols
        let odoo = Self {
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
        };
        odoo
    }

    pub async fn init(&mut self, client: &Client) {
        client.log_message(MessageType::INFO, "Building new Odoo knowledge database").await;
        let response = client.send_request::<ConfigRequest>(()).await.unwrap();
        self.config.addons = response.addons.clone();
        self.config.odoo_path = response.odoo_path.clone();
        self.config.python_path = response.python_path.clone();
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
        if let Some(map) = config.as_object() {
            for (key, value) in map {
                match key.as_str() {
                    "autoRefresh" => {
                        if let Some(refresh_mode) = value.as_str() {
                            self.config.refresh_mode = match RefreshMode::from_str(refresh_mode) {
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
                            self.config.auto_save_delay = refresh_delay;
                        } else {
                            client.log_message(MessageType::ERROR, "Unable to parse auto_save_delay. Setting it to 2000").await;
                            self.config.auto_save_delay = 2000
                        }
                    },
                    "diagMissingImportLevel" => {
                        if let Some(diag_import_level) = value.as_str() {
                            self.config.diag_missing_imports = match DiagMissingImportsMode::from_str(diag_import_level) {
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
        {
            let mut root_symbol = self.symbols.as_ref().unwrap().lock().unwrap();
            root_symbol.paths.push(self.stdlib_dir.clone());
            root_symbol.paths.push(self.stubs_dir.clone());
            //TODO add sys.path
            let output = Command::new(self.config.python_path.clone()).args(&["-c", "import sys; print(sys.path)"]).output().expect("Can't exec python3");
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
        let _builtins_arc_symbol = match self.builtins {
            Some(ref builtins) => {
                let mut b = builtins.lock().unwrap();
                let _builtins_symbol = Symbol::create_from_path(&builtins_path, &b, false);
                b.add_symbol(self, _builtins_symbol.unwrap())
            },
            None => panic!("Builtins symbol not found")
        };
        self.add_to_rebuild_arch(Arc::downgrade(&_builtins_arc_symbol));
        self.process_rebuilds(&client).await;
    }

    async fn build_database(&mut self, client: &Client) {
        client.log_message(MessageType::INFO, "Building Database").await;
        let result = self.build_base(client).await;
        if result {
            self.build_modules(client).await;
        }
    }

    async fn build_base(&mut self, client: &Client) -> bool {
        let release_path = PathBuf::from(self.config.odoo_path.clone()).join("odoo/release.py");
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
        for line in release_file.lines() {
            if line.starts_with("version_info = (") {
                let re = Regex::new(r#"version_info = \((['\"]?(\D+~)?\d+['\"]?, \d+, \d+, \w+, \d+, \D+)\)"#).unwrap();
                let result = re.captures(line);
                match result {
                    Some(result) => {
                        let version_info = result.get(1).unwrap().as_str();
                        let version_info = version_info.split(", ").collect::<Vec<&str>>();
                        let version_major = version_info[0].replace("saas~", "").replace("'", "").replace(r#"""#, "");
                        self.version_major = version_major.parse().unwrap();
                        self.version_minor = version_info[1].parse().unwrap();
                        self.version_micro = version_info[2].parse().unwrap();
                        self.full_version = format!("{}.{}.{}", self.version_major, self.version_minor, self.version_micro);
                        break;
                    },
                    None => {
                        self.version_major = 14;
                        self.version_minor = 0;
                        self.version_micro = 0;
                        client.log_message(MessageType::ERROR, "Unable to detect the Odoo version. Running the tool for the version 14").await;
                        return false;
                    }
                }
            }
        }
        client.log_message(MessageType::INFO, format!("Odoo version: {}", self.full_version)).await;
        if self.version_major < 14 {
            client.log_message(MessageType::ERROR, "Odoo version is less than 14. The tool only supports version 14 and above. Aborting...").await;
            return false;
        }
        //build base
        self.symbols.as_ref().unwrap().lock().unwrap().paths.push(self.config.odoo_path.clone());
        let _odoo_arc_symbol = match self.symbols {
            Some(ref symbols) => {
                let mut s = symbols.lock().unwrap();
                let _odoo_symbol = Symbol::create_from_path(&PathBuf::from(self.config.odoo_path.clone()).join("odoo"), &s, false);
                s.add_symbol(self, _odoo_symbol.unwrap())
            },
            None => panic!("Odoo root symbol not found")
        };
        self.add_to_rebuild_arch(Arc::downgrade(&_odoo_arc_symbol));
        self.process_rebuilds(&client).await;
        //search common odoo addons path
        let addon_symbol = self.get_symbol(&tree(vec!["odoo", "addons"], vec![]));
        let odoo_addon_path = PathBuf::from(self.config.odoo_path.clone()).join("addons");
        if odoo_addon_path.exists() {
            addon_symbol.as_ref().unwrap().lock().unwrap().paths.push(
                odoo_addon_path.to_str().unwrap().to_string()
            );
        } else {
            client.log_message(MessageType::ERROR, format!("Unable to find odoo addons path at {}", odoo_addon_path.to_str().unwrap().to_string())).await;
            return false;
        }
        return true
    }

    async fn build_modules(&mut self, client: &Client) {
        {
            let addons_symbol = self.get_symbol(&tree(vec!["odoo", "addons"], vec![])).expect("Unable to find odoo addons symbol");
            let addons_path = &addons_symbol.lock().unwrap().paths;
            for addon_path in addons_path.iter() {
                if PathBuf::from(addon_path).exists() {
                    //browse all dir in path
                    for item in PathBuf::from(addon_path).read_dir().expect("Unable to find odoo addons path") {
                        match item {
                            Ok(item) => {
                                if item.file_type().unwrap().is_dir() {
                                    let mut a_m = addons_symbol.lock().unwrap();
                                    let module_symbol = Symbol::create_from_path(&item.path(), &a_m, true);
                                    let _odoo_arc_symbol = a_m.add_symbol(self, module_symbol.unwrap());
                                    self.add_to_rebuild_arch(Arc::downgrade(&_odoo_arc_symbol));
                                }
                            },
                            Err(_) => {}
                        }
                    }

                }
            }
        }
        self.process_rebuilds(client).await;
        client.log_message(MessageType::INFO, "End building modules.").await;
    }

    pub fn get_symbol(&self, tree: &Tree) -> Option<Arc<Mutex<Symbol>>> {
        self.symbols.as_ref().unwrap().lock().unwrap().get_symbol(&tree)
    }

    async fn pop_item(&mut self, step: BuildSteps) -> Option<Arc<Mutex<Symbol>>> {
        let mut arc_sym: Option<Arc<Mutex<Symbol>>> = None;
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
            let mut selected_sym: Option<&MyWeak<Mutex<Symbol>>> = None;
            let mut selected_count: u32 = 999999999;
            let mut current_count: u32;
            for sym in &*set {
                current_count = 0;
                let myt_symbol = sym.upgrade().unwrap();
                let symbol = myt_symbol.lock().unwrap();
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
            if selected_sym.is_none() {
                return None;
            }
            arc_sym = selected_sym.unwrap().upgrade()
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
            let arc_sym_unwrapped = arc_sym.unwrap();
            if !set.remove(&MyWeak::new(Arc::downgrade(&arc_sym_unwrapped))) {
                panic!("Unable to remove selected symbol from rebuild set")
            }
            return Some(arc_sym_unwrapped);
        }
    }

    async fn process_rebuilds(&mut self, client: &Client) {
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::new();
        while !self.rebuild_arch.is_empty() || !self.rebuild_arch_eval.is_empty() {
            let sym = self.pop_item(BuildSteps::ARCH).await;
            if sym.is_some() {
                let sym_arc = sym.as_ref().unwrap().clone();
                let tree = sym_arc.lock().unwrap().get_tree().clone();
                if already_arch_rebuilt.contains(&tree) {
                    println!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchBuilder::new(sym_arc);
                builder.load_arch(self).await;
                continue;
            }
            let sym = self.pop_item(BuildSteps::ARCH_EVAL).await;
            if sym.is_some() {
                let sym_arc = sym.as_ref().unwrap().clone();
                let tree = sym_arc.lock().unwrap().get_tree().clone();
                if already_arch_eval_rebuilt.contains(&tree) {
                    println!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchEval::new(sym_arc);
                builder.eval_arch(self).await;
                continue;
            }
        }
    }

    pub fn add_to_rebuild_arch(&mut self, symbol: Weak<Mutex<Symbol>>) {
        self.rebuild_arch.insert(MyWeak::new(symbol));
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: Weak<Mutex<Symbol>>) {
        self.rebuild_arch_eval.insert(MyWeak::new(symbol));
    }
}