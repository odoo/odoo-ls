use crate::core::config::{Config, ConfigRequest};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;

use std::collections::HashSet;
use std::ops::Deref;
use std::sync::{Arc, Weak, Mutex};
use std::str::FromStr;
use std::fs;
use std::path::PathBuf;
use std::thread::current;
use crate::constants::*;
use super::config::{RefreshMode, DiagMissingImportsMode};
use super::symbol::Symbol;
use crate::my_weak::MyWeak;
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
    rebuild_arch: HashSet<MyWeak<Mutex<Symbol>>>,
    rebuild_arch_eval: HashSet<MyWeak<Mutex<Symbol>>>,
    rebuild_odoo: HashSet<MyWeak<Mutex<Symbol>>>,
    rebuild_validation: HashSet<MyWeak<Mutex<Symbol>>>,
}

impl Odoo {
    pub fn new() -> Self {
        Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            config: Config::new(),
            symbols: Some(Arc::new(Mutex::new(Symbol::new("root".to_string(), SymType::ROOT)))),
            builtins: Some(Arc::new(Mutex::new(Symbol::new("builtins".to_string(), SymType::ROOT)))),
            rebuild_arch: HashSet::new(),
            rebuild_arch_eval: HashSet::new(),
            rebuild_odoo: HashSet::new(),
            rebuild_validation: HashSet::new(),
        }
    }

    pub async fn init(&mut self, client: &Client) {
        client.log_message(MessageType::INFO, "Building new Odoo knowledge database").await;
        let response = client.send_request::<ConfigRequest>(()).await.unwrap();
        self.config.addons = response.addons.clone();
        self.config.odoo_path = response.odoo_path.clone();
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
                        client.log_message(MessageType::ERROR, "Unknown config key: {key}").await;
                    },
                }
            }
        }
        self.load_builtins(client).await;
    }

    async fn load_builtins(&mut self, client: &Client) {
        let builtins_path = fs::canonicalize(PathBuf::from("./typeshed/stdlib/builtins.pyi"));
        let Ok(builtins_path) = builtins_path else {
            client.log_message(MessageType::ERROR, "Unable to find builtins.pyi").await;
            return;
        };
        let arc_symbol = Arc::new(Mutex::new(Symbol::create_from_path(builtins_path.to_str().unwrap(), self.builtins.unwrap()).unwrap()));
        self.add_to_rebuild_arch(Arc::downgrade(&arc_symbol));
        self.process_rebuilds(client).await;
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
                for (index, dep_set) in symbol.get_all_dependencies(&step).iter().enumerate() {
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
        //already_rebuilt: HashSet<tree>;// TODO track already rebuilt to avoid cycles
        while !self.rebuild_arch.is_empty() {
            let sym = self.pop_item(BuildSteps::ARCH).await;
            if sym.is_none() {
                break;
            }
        }
    }

    fn add_to_rebuild_arch(&mut self, symbol: Weak<Mutex<Symbol>>) {
        self.rebuild_arch.insert(MyWeak::new(symbol));
    }
}