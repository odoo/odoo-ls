use crate::core::config::{Config, ConfigRequest};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;

use std::str::FromStr;
use std::fs;
use std::path::PathBuf;
use super::config::{RefreshMode, DiagMissingImportsMode};
use super::symbol::Symbol;
use super::python_arch_builder::PythonArchBuilder;

#[derive(Debug)]
pub struct Odoo {
    pub version_major: u32,
    pub version_minor: u32,
    pub version_micro: u32,
    pub full_version: String,
    pub config: Config,
    pub symbols: Box<Symbol>, //TODO: Pin ?
    pub builtins: Box<Symbol>,
}

impl Odoo {
    pub fn new() -> Self {
        Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            config: Config::new(),
            symbols: Box::new(Symbol::new()),
            builtins: Box::new(Symbol::new()),
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

    async fn load_builtins(&self, client: &Client) {
        let builtins_path = fs::canonicalize(PathBuf::from("./typeshed/stdlib/builtins.pyi"));
        if builtins_path.is_err() {
            client.log_message(MessageType::ERROR, "Unable to find builtins.pyi").await;
            return;
        }
        let builtins_path = builtins_path.unwrap();
        let builder = PythonArchBuilder::new();
        builder.load_arch();
        self.process_rebuilds(client).await;
    }

    async fn process_rebuilds(&self, client: &Client) {
        
    }
}