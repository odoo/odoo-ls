use std::collections::{hash_map, HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, path::Path, error::Error};
use serde::Deserialize;

use crate::utils::{fill_validate_path, is_addon_path, is_odoo_path, is_python_path, PathSanitizer};

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
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
    pub file_cache: bool,
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
            file_cache: true,
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

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
enum MergeMethod {
    Merge,
    Override,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    config: Vec<ConfigEntryRaw>,
}

// Raw structure for initial deserialization
#[derive(Debug, Deserialize)]
struct ConfigEntryRaw {
    #[serde(default = "default_name")]
    name: String,

    #[serde(default)]
    extends: Option<String>, // Allowed to extend from another config

    #[serde(default)]
    odoo_path: Option<String>,

    #[serde(default = "default_addons_merge")]
    addons_merge: MergeMethod,

    #[serde(default)]
    addons_paths: Vec<String>,

    #[serde(default)]
    python_path: Option<String>,

    #[serde(default)]
    additional_stubs: Vec<String>,

    #[serde(default = "default_addons_merge")]
    additional_stubs_merge: MergeMethod,

    #[serde(default)]
    refresh_mode: Option<RefreshMode>,
}
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    odoo_path: Option<PathBuf>,
    addons_paths: Vec<String>,
    addons_merge: MergeMethod,
    python_path: Option<String>,
    additional_stubs: Vec<String>,
    additional_stubs_merge: MergeMethod,
    refresh_mode: RefreshMode,
}
pub type ConfigNew = HashMap<String, ConfigEntry>;


fn default_name() -> String {
    "root".to_string()
}

fn default_addons_merge() -> MergeMethod {
    MergeMethod::Merge
}

fn read_config_from_file<P: AsRef<Path>>(ws_folders: hash_map::Iter<String, String>, path: P) -> Result<ConfigNew, Box<dyn Error>> {
    let path = path.as_ref();
    let config_dir = path.parent().unwrap_or(Path::new("."));
    let contents = fs::read_to_string(path)?;
    let raw: ConfigFile = toml::from_str(&contents)?;

    let config = raw.config.into_iter().map(|entry| {
        let odoo_path = entry.odoo_path
            .and_then(|p| fill_validate_path(ws_folders.clone(), &p, is_odoo_path))
            .map(|p| config_dir.join(p));
        let addons_paths = entry.addons_paths.iter()
            .filter_map(|p| fill_validate_path(ws_folders.clone(), p, is_addon_path))
            .map(|p| config_dir.join(p).sanitize())
            .collect::<Vec<_>>();
        let python_path = entry.python_path
            .and_then(|p| fill_validate_path(ws_folders.clone(), &p, is_python_path))
            .map(|p| config_dir.join(p).sanitize());
        (entry.name, ConfigEntry {
            odoo_path,
            addons_paths: addons_paths,
            addons_merge: entry.addons_merge,
            python_path: python_path,
            additional_stubs: entry.additional_stubs,
            additional_stubs_merge: entry.additional_stubs_merge,
            refresh_mode: entry.refresh_mode.unwrap_or(RefreshMode::Adaptive), // Default to Adaptive
        })
    }).collect();

    Ok(config)
}
fn merge_configs(child: &ConfigNew, parent: &ConfigNew) -> ConfigNew {
    let mut merged = HashMap::new();

    // Collect all keys from both child and parent
    let keys: std::collections::HashSet<_> = child.keys()
        .chain(parent.keys())
        .cloned()
        .collect();

    for key in keys {
        let child_entry = child.get(&key);
        let parent_entry = parent.get(&key);

        let entry = match (child_entry, parent_entry) {
            (Some(child), Some(parent)) => {
                let odoo_path = if child.odoo_path.is_some() {
                    child.odoo_path.clone()
                } else {
                    parent.odoo_path.clone()
                };

                let python_path = if child.python_path.is_some() {
                    child.python_path.clone()
                } else {
                    parent.python_path.clone()
                };

                let addons_paths = match child.addons_merge {
                    MergeMethod::Merge => {
                        let mut merged_paths = parent.addons_paths.clone();
                        merged_paths.extend(child.addons_paths.clone());
                        merged_paths
                    },
                    MergeMethod::Override => {
                        child.addons_paths.clone()
                    }
                };

                let additional_stubs = match child.additional_stubs_merge {
                    MergeMethod::Merge => {
                        let mut merged_stubs = parent.additional_stubs.clone();
                        merged_stubs.extend(child.additional_stubs.clone());
                        merged_stubs
                    },
                    MergeMethod::Override => {
                        child.additional_stubs.clone()
                    }
                };

                let refresh_mode = if child.refresh_mode != RefreshMode::Adaptive {
                    child.refresh_mode.clone()
                } else {
                    parent.refresh_mode.clone()
                };

                ConfigEntry {
                    odoo_path,
                    addons_paths,
                    addons_merge: child.addons_merge.clone(),
                    python_path,
                    additional_stubs,
                    additional_stubs_merge: child.additional_stubs_merge.clone(),
                    refresh_mode,
                }
            }
            (Some(child), None) => child.clone(),
            (None, Some(parent)) => parent.clone(),
            (None, None) => continue, // unreachable
        };

        merged.insert(key, entry);
    }

    merged
}


pub fn load_merged_config_upward(ws_folders: hash_map::Iter<String, String>, start: &String) -> Result<ConfigNew, Box<dyn Error>> {
    let mut current_dir = PathBuf::from(start);
    let mut visited_dirs = HashSet::new();
    let mut merged_config: ConfigNew = HashMap::new();

    loop {
        if !visited_dirs.insert(current_dir.clone()) {
            break; // prevent loops (e.g. via symlinks)
        }

        let config_path = current_dir.join("odools.toml");
        if config_path.exists() && config_path.is_file() {
            let current_config = read_config_from_file(ws_folders.clone(), &config_path)?;
            merged_config = merge_configs(&current_config, &merged_config);
        }

        // Stop if we’re at the root
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    Ok(merged_config)
}
