use std::collections::{hash_map, HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, path::Path, error::Error};
use itertools::Itertools;
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

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigEntryRaw {
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

    #[serde(default)]
    file_cache: Option<bool>,

    #[serde(default)]
    diag_missing_imports: Option<DiagMissingImportsMode>,

    #[serde(default)]
    ac_filter_model_names: Option<bool>,
}
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    odoo_path: Option<String>,
    addons_paths: Vec<String>,
    addons_merge: MergeMethod,
    python_path: Option<String>,
    additional_stubs: Vec<String>,
    additional_stubs_merge: MergeMethod,
    refresh_mode: RefreshMode,
    file_cache: bool,
    diag_missing_imports: DiagMissingImportsMode,
    ac_filter_model_names: bool,
    extends: Option<String>, // Added extends field
}
pub type ConfigNew = HashMap<String, ConfigEntry>;


fn default_name() -> String {
    "root".to_string()
}

fn default_addons_merge() -> MergeMethod {
    MergeMethod::Merge
}

fn read_config_from_file<P: AsRef<Path>>(ws_folders: hash_map::Iter<String, String>, path: P) -> Result<HashMap<String, ConfigEntryRaw>, Box<dyn Error>> {
    let path = path.as_ref();
    let config_dir = path.parent().unwrap_or(Path::new("."));
    let contents = fs::read_to_string(path)?;
    let raw: ConfigFile = toml::from_str(&contents)?;

    let config = raw.config.into_iter().map(|mut entry| {
        // Validate and sanitize paths, but keep them as `Option` to preserve emptiness
        entry.odoo_path = entry.odoo_path
            .map(|p| fill_validate_path(ws_folders.clone(), &p, is_odoo_path).unwrap_or(p))
            .and_then(|p| std::fs::canonicalize(config_dir.join(p)).ok())
            .map(|p| p.sanitize())
            .filter(|p| is_odoo_path(p));

        entry.addons_paths = entry.addons_paths.into_iter()
            .map(|p| fill_validate_path(ws_folders.clone(), &p, is_addon_path).unwrap_or(p))
            .flat_map(|p| std::fs::canonicalize(config_dir.join(p)).ok())
            .map(|p| p.sanitize())
            .filter(|p| is_addon_path(p))
            .unique()
            .collect();

        entry.python_path = entry.python_path
            .map(|p| fill_validate_path(ws_folders.clone(), &p, is_python_path).unwrap_or(p))
            .and_then(|p| std::fs::canonicalize(config_dir.join(p)).ok())
            .map(|p| p.sanitize())
            .filter(|p| is_python_path(p));

        (entry.name.clone(), entry)
    }).collect();

    Ok(config)
}

fn apply_extends(mut config: HashMap<String, ConfigEntryRaw>) -> HashMap<String, ConfigEntryRaw> {
    let keys: Vec<String> = config.keys().cloned().collect();
    for key in keys {
        if let Some(entry) = config.get(&key).cloned() {
            if let Some(extends_key) = entry.extends.clone() {
                if let Some(parent_entry) = config.get(&extends_key).cloned() {
                    let merged_entry = ConfigEntryRaw {
                        odoo_path: entry.odoo_path.or(parent_entry.odoo_path),
                        python_path: entry.python_path.or(parent_entry.python_path),
                        addons_paths: match entry.addons_merge {
                            MergeMethod::Merge => {
                                let mut merged_paths = parent_entry.addons_paths.clone();
                                merged_paths.extend(entry.addons_paths);
                                merged_paths
                            },
                            MergeMethod::Override => entry.addons_paths,
                        },
                        additional_stubs: match entry.additional_stubs_merge {
                            MergeMethod::Merge => {
                                let mut merged_stubs = parent_entry.additional_stubs.clone();
                                merged_stubs.extend(entry.additional_stubs);
                                merged_stubs
                            },
                            MergeMethod::Override => entry.additional_stubs,
                        },
                        refresh_mode: entry.refresh_mode.or(parent_entry.refresh_mode),
                        file_cache: entry.file_cache.or(parent_entry.file_cache),
                        diag_missing_imports: entry.diag_missing_imports.or(parent_entry.diag_missing_imports),
                        ac_filter_model_names: entry.ac_filter_model_names.or(parent_entry.ac_filter_model_names),
                        addons_merge: entry.addons_merge,
                        additional_stubs_merge: entry.additional_stubs_merge,
                        extends: entry.extends,
                        name: entry.name,
                    };
                    config.insert(key, merged_entry);
                }
            }
        }
    }
    config
}

fn merge_configs(
    child: &HashMap<String, ConfigEntryRaw>,
    parent: &HashMap<String, ConfigEntryRaw>,
) -> HashMap<String, ConfigEntryRaw> {
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
                let odoo_path = child.odoo_path.clone().or(parent.odoo_path.clone());
                let python_path = child.python_path.clone().or(parent.python_path.clone());
                let addons_paths = match child.addons_merge {
                    MergeMethod::Merge => {
                        let mut merged_paths = parent.addons_paths.clone();
                        merged_paths.extend(child.addons_paths.clone());
                        merged_paths
                    },
                    MergeMethod::Override => child.addons_paths.clone(),
                };
                let additional_stubs = match child.additional_stubs_merge {
                    MergeMethod::Merge => {
                        let mut merged_stubs = parent.additional_stubs.clone();
                        merged_stubs.extend(child.additional_stubs.clone());
                        merged_stubs
                    },
                    MergeMethod::Override => child.additional_stubs.clone(),
                };
                let refresh_mode = child.refresh_mode.clone().or(parent.refresh_mode.clone());
                let file_cache = child.file_cache.clone().or(parent.file_cache.clone());
                let diag_missing_imports = child.diag_missing_imports.clone().or(parent.diag_missing_imports.clone());
                let ac_filter_model_names = child.ac_filter_model_names.clone().or(parent.ac_filter_model_names.clone());
                let extends = child.extends.clone().or(parent.extends.clone());

                ConfigEntryRaw {
                    odoo_path,
                    python_path,
                    addons_paths,
                    additional_stubs,
                    refresh_mode,
                    file_cache,
                    diag_missing_imports,
                    ac_filter_model_names,
                    addons_merge: child.addons_merge.clone(),
                    additional_stubs_merge: child.additional_stubs_merge.clone(),
                    extends,
                    name: child.name.clone(),
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


pub fn load_merged_config_upward(ws_folders: hash_map::Iter<String, String>, start: &String) -> Result<HashMap<String, ConfigEntryRaw>, Box<dyn Error>> {
    let mut current_dir = PathBuf::from(start);
    let mut visited_dirs = HashSet::new();
    let mut merged_config: HashMap<String, ConfigEntryRaw> = HashMap::new();

    loop {
        if !visited_dirs.insert(current_dir.clone()) {
            break; // prevent loops (e.g. via symlinks)
        }

        let config_path = current_dir.join("odools.toml");
        if config_path.exists() && config_path.is_file() {
            let current_config = read_config_from_file(ws_folders.clone(), &config_path)?;
            merged_config = merge_configs(&current_config, &merged_config);
            merged_config = apply_extends(merged_config);
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

pub fn merge_all_workspaces(
    workspace_configs: Vec<HashMap<String, ConfigEntryRaw>>,
) -> Result<ConfigNew, String> {
    let mut merged_config: HashMap<String, ConfigEntry> = HashMap::new();

    for workspace_config in workspace_configs {
        for (key, raw_entry) in workspace_config {
            let merged_entry = merged_config.entry(key.clone()).or_insert_with(|| ConfigEntry {
                odoo_path: None,
                addons_paths: vec![],
                addons_merge: MergeMethod::Merge,
                python_path: None,
                additional_stubs: vec![],
                additional_stubs_merge: MergeMethod::Merge,
                refresh_mode: RefreshMode::Adaptive,
                file_cache: true,
                diag_missing_imports: DiagMissingImportsMode::All,
                ac_filter_model_names: true,
                extends: None,
            });

            // Check for conflicts in odoo_path
            if let (Some(existing), Some(new)) = (&merged_entry.odoo_path, &raw_entry.odoo_path) {
                if existing != new {
                    return Err(format!(
                        "Conflict detected in 'odoo_path' for key '{}': '{}' vs '{}'",
                        key, existing, new
                    ));
                }
            }

            // Merge fields
            merged_entry.odoo_path = match (&merged_entry.odoo_path, &raw_entry.odoo_path) {
                (Some(existing), Some(new)) if existing == new => Some(existing.clone()),
                (None, Some(new)) | (Some(new), None) => Some(new.clone()),
                _ => merged_entry.odoo_path.clone(),
            };

            merged_entry.python_path = match (&merged_entry.python_path, &raw_entry.python_path) {
                (Some(existing), Some(new)) if existing == new => Some(existing.clone()),
                (None, Some(new)) | (Some(new), None) => Some(new.clone()),
                _ => merged_entry.python_path.clone(),
            };

            merged_entry.addons_paths = match raw_entry.addons_merge {
                MergeMethod::Merge => {
                    let mut merged_paths = merged_entry.addons_paths.clone();
                    merged_paths.extend(raw_entry.addons_paths.clone());
                    merged_paths
                }
                MergeMethod::Override => raw_entry.addons_paths.clone(),
            };

            merged_entry.additional_stubs = match raw_entry.additional_stubs_merge {
                MergeMethod::Merge => {
                    let mut merged_stubs = merged_entry.additional_stubs.clone();
                    merged_stubs.extend(raw_entry.additional_stubs.clone());
                    merged_stubs
                }
                MergeMethod::Override => raw_entry.additional_stubs.clone(),
            };

            merged_entry.refresh_mode = match (&merged_entry.refresh_mode, &raw_entry.refresh_mode) {
                (RefreshMode::Adaptive, Some(new)) => new.clone(),
                (_, None) => merged_entry.refresh_mode.clone(),
                (_, Some(new)) => new.clone(),
            };

            merged_entry.file_cache = merged_entry.file_cache && raw_entry.file_cache.unwrap_or(true);

            merged_entry.diag_missing_imports = match (
                &merged_entry.diag_missing_imports,
                &raw_entry.diag_missing_imports,
            ) {
                (DiagMissingImportsMode::All, Some(new)) => new.clone(),
                (_, None) => merged_entry.diag_missing_imports.clone(),
                (_, Some(new)) => new.clone(),
            };

            merged_entry.ac_filter_model_names =
                merged_entry.ac_filter_model_names && raw_entry.ac_filter_model_names.unwrap_or(true);

            merged_entry.extends = match (&merged_entry.extends, &raw_entry.extends) {
                (Some(existing), Some(new)) if existing == new => Some(existing.clone()),
                (None, Some(new)) | (Some(new), None) => Some(new.clone()),
                _ => merged_entry.extends.clone(),
            };

            merged_entry.addons_merge = match (&merged_entry.addons_merge, &raw_entry.addons_merge) {
                (MergeMethod::Merge, MergeMethod::Merge) => MergeMethod::Merge,
                (_, new) => new.clone(),
            };

            merged_entry.additional_stubs_merge = match (
                &merged_entry.additional_stubs_merge,
                &raw_entry.additional_stubs_merge,
            ) {
                (MergeMethod::Merge, MergeMethod::Merge) => MergeMethod::Merge,
                (_, new) => new.clone(),
            };
        }
    }

    Ok(merged_config)
}
