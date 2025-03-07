use std::str::FromStr;
use lsp_types::request::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone)]
pub enum RefreshMode {
    OnSave,
    Adaptive,
    Off
}

impl FromStr for RefreshMode {

    type Err = ();

    fn from_str(input: &str) -> Result<RefreshMode, Self::Err> {
        match input {
            "afterDelay"  => Ok(RefreshMode::Adaptive),
            "onSave"  => Ok(RefreshMode::OnSave),
            "adaptive" => Ok(RefreshMode::Adaptive),
            "off"  => Ok(RefreshMode::Off),
            _      => Err(()),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum DiagMissingImportsMode {
    None,
    OnlyOdoo,
    All
}

impl FromStr for DiagMissingImportsMode {

    type Err = ();

    fn from_str(input: &str) -> Result<DiagMissingImportsMode, Self::Err> {
        match input {
            "none"  => Ok(DiagMissingImportsMode::None),
            "only_odoo"  => Ok(DiagMissingImportsMode::OnlyOdoo),
            "all"  => Ok(DiagMissingImportsMode::All),
            _      => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub refresh_mode: RefreshMode,
    pub auto_save_delay: u64,
    pub diag_missing_imports: DiagMissingImportsMode,
    pub diag_only_opened_files: bool,
    pub addons: Vec<String>,
    pub odoo_path: Option<String>,
    pub python_path: String,
    pub no_typeshed: bool,
    pub additional_stubs: Vec<String>,
    pub stdlib: String,
    pub ac_filter_model_names: bool, // AC: Only show model names from module dependencies 
}

impl Config {
    pub fn new() -> Self {
        Self {
            refresh_mode: RefreshMode::Adaptive,
            auto_save_delay: 1000,
            diag_missing_imports: DiagMissingImportsMode::All,
            diag_only_opened_files: false,
            addons: Vec::new(),
            odoo_path: None,
            python_path: "python3".to_string(),
            no_typeshed: false,
            additional_stubs: vec![],
            stdlib: "".to_string(),
            ac_filter_model_names: false,
        }
    }
}