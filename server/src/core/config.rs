use std::str::FromStr;
use tower_lsp::lsp_types::request::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
pub enum RefreshMode {
    AfterDelay,
    OnSave,
    Off
}

impl FromStr for RefreshMode {

    type Err = ();

    fn from_str(input: &str) -> Result<RefreshMode, Self::Err> {
        match input {
            "afterDelay"  => Ok(RefreshMode::AfterDelay),
            "onSave"  => Ok(RefreshMode::OnSave),
            "off"  => Ok(RefreshMode::Off),
            _      => Err(()),
        }
    }
}

#[derive(Debug, PartialEq)]
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

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRequestResult {
    pub addons: Vec<String>,
    pub id: u32,
    pub name: String,
    pub odoo_path: String,
    pub python_path: String,
}

#[derive(Debug)]
pub enum ConfigRequest {}

impl Request for ConfigRequest {
    type Params = ();
    type Result = ConfigRequestResult;
    const METHOD: &'static str = "Odoo/getConfiguration";
}

#[derive(Debug)]
pub struct Config {
    pub refresh_mode: RefreshMode,
    pub auto_save_delay: u64,
    pub diag_missing_imports: DiagMissingImportsMode,
    pub addons: Vec<String>,
    pub odoo_path: String,
    pub python_path: String,
}

impl Config {
    pub fn new() -> Self {
        Self {
            refresh_mode: RefreshMode::AfterDelay,
            auto_save_delay: 1000,
            diag_missing_imports: DiagMissingImportsMode::All,
            addons: Vec::new(),
            odoo_path: "".to_string(),
            python_path: "".to_string(),
        }
    }
}