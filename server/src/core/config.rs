use std::str::FromStr;
use lsp_types::request::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone)]
pub enum RefreshMode {
    AfterDelay,
    OnSave,
    Adaptive,
    Off
}

impl FromStr for RefreshMode {

    type Err = ();

    fn from_str(input: &str) -> Result<RefreshMode, Self::Err> {
        match input {
            "afterDelay"  => Ok(RefreshMode::AfterDelay),
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

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonPathRequestResult {
    pub python_path: String,
}

#[derive(Debug)]
pub enum PythonPathRequest {}

impl Request for PythonPathRequest {
    type Params = ();
    type Result = PythonPathRequestResult;
    const METHOD: &'static str = "Odoo/getPythonPath";
}

#[derive(Debug, Clone)]
pub struct Config {
    pub refresh_mode: RefreshMode,
    pub auto_save_delay: u64,
    pub diag_missing_imports: DiagMissingImportsMode,
    pub diag_only_opened_files: bool,
    pub addons: Vec<String>,
    pub odoo_path: String,
    pub python_path: String,
    pub no_typeshed: bool,
    pub additional_stubs: Vec<String>,
    pub stdlib: String,
}

impl Config {
    pub fn new() -> Self {
        Self {
            refresh_mode: RefreshMode::Adaptive,
            auto_save_delay: 1000,
            diag_missing_imports: DiagMissingImportsMode::All,
            diag_only_opened_files: false,
            addons: Vec::new(),
            odoo_path: "".to_string(),
            python_path: "python3".to_string(),
            no_typeshed: false,
            additional_stubs: vec![],
            stdlib: "".to_string(),
        }
    }
}