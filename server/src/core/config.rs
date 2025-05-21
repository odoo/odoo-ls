use std::collections::{hash_map, HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, path::Path, error::Error};
use itertools::Itertools;
use serde::Deserialize;

use crate::utils::{fill_validate_path, is_addon_path, is_odoo_path, is_python_path, PathSanitizer};
use crate::S;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum RefreshMode {
    OnSave,
    Adaptive,
    Off
}
impl Default for RefreshMode {
    fn default() -> Self {
        RefreshMode::Adaptive
    }
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
impl Default for DiagMissingImportsMode {
    fn default() -> Self {
        DiagMissingImportsMode::All
    }
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

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum MergeMethod {
    Merge,
    Override,
}

impl Default for MergeMethod {
    fn default() -> Self {
        MergeMethod::Merge
    }
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

    #[serde(default)]
    addons_merge: Option<MergeMethod>,

    #[serde(default)]
    addons_paths: Vec<String>,

    #[serde(default)]
    python_path: Option<String>,

    #[serde(default)]
    additional_stubs: Vec<String>,

    #[serde(default)]
    additional_stubs_merge: Option<MergeMethod>,

    #[serde(default)]
    refresh_mode: Option<RefreshMode>,

    #[serde(default)]
    file_cache: Option<bool>,

    #[serde(default)]
    diag_missing_imports: Option<DiagMissingImportsMode>,

    #[serde(default)]
    ac_filter_model_names: Option<bool>,

    #[serde(default)]
    auto_save_delay: Option<u64>,

    #[serde(default)]
    add_workspace_addon_path: Option<bool>,
}
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub odoo_path: Option<String>,
    pub addons_paths: Vec<String>,
    pub python_path: String,
    pub additional_stubs: Vec<String>,
    pub refresh_mode: RefreshMode,
    pub file_cache: bool,
    pub diag_missing_imports: DiagMissingImportsMode,
    pub ac_filter_model_names: bool,
    pub auto_save_delay: u64,
    pub extends: Option<String>, // Added extends field
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
                        addons_paths: match entry.addons_merge.unwrap_or_default() {
                            MergeMethod::Merge => {
                                let mut merged_paths = parent_entry.addons_paths.clone();
                                merged_paths.extend(entry.addons_paths);
                                merged_paths
                            },
                            MergeMethod::Override => entry.addons_paths,
                        },
                        additional_stubs: match entry.additional_stubs_merge.unwrap_or_default() {
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
                        addons_merge: entry.addons_merge.or(parent_entry.addons_merge),
                        additional_stubs_merge: entry.additional_stubs_merge.or(parent_entry.additional_stubs_merge),
                        extends: entry.extends,
                        name: entry.name,
                        auto_save_delay: entry.auto_save_delay.or(parent_entry.auto_save_delay),
                        add_workspace_addon_path: entry.add_workspace_addon_path.or(parent_entry.add_workspace_addon_path),
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
                let addons_paths = match child.addons_merge.unwrap_or_default() {
                    MergeMethod::Merge => parent.addons_paths.clone().into_iter().chain(child.addons_paths.clone().into_iter()).unique().collect(),
                    MergeMethod::Override => child.addons_paths.clone(),
                };
                let additional_stubs = match child.additional_stubs_merge.unwrap_or_default() {
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
                let addons_merge = child.addons_merge.or(parent.addons_merge);
                let additional_stubs_merge = child.additional_stubs_merge.or(parent.additional_stubs_merge);
                let extends = child.extends.clone().or(parent.extends.clone());
                let auto_save_delay = child.auto_save_delay.or(parent.auto_save_delay);
                let add_workspace_addon_path = child.add_workspace_addon_path.or(parent.add_workspace_addon_path);

                ConfigEntryRaw {
                    odoo_path,
                    python_path,
                    addons_paths,
                    additional_stubs,
                    refresh_mode,
                    file_cache,
                    diag_missing_imports,
                    ac_filter_model_names,
                    addons_merge,
                    additional_stubs_merge,
                    extends,
                    name: child.name.clone(),
                    auto_save_delay,
                    add_workspace_addon_path,
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
            merged_config = merge_configs(&merged_config, &current_config);
            merged_config = apply_extends(merged_config);
        }

        // Stop if we’re at the root
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    for (_, entry) in merged_config.iter_mut() {
        if (matches!(entry.add_workspace_addon_path, Some(true)) || entry.addons_paths.is_empty()) && is_addon_path(start) {
            entry.addons_paths.push(start.clone());
        }
    }

    Ok(merged_config)
}

pub fn merge_all_workspaces(
    workspace_configs: Vec<HashMap<String, ConfigEntryRaw>>,
    ws_folders: hash_map::Iter<String, String>
) -> Result<ConfigNew, String> {
    let mut merged_raw_config: HashMap<String, ConfigEntryRaw> = HashMap::new();

    // First, merge all workspace configurations into a ConfigEntryRaw structure
    for workspace_config in workspace_configs {
        for (key, raw_entry) in workspace_config {
            let merged_entry = merged_raw_config.entry(key.clone()).or_insert_with(|| ConfigEntryRaw {
                name: key.clone(),
                extends: None,
                odoo_path: None,
                addons_merge: None,
                addons_paths: vec![],
                python_path: None,
                additional_stubs: vec![],
                additional_stubs_merge: None,
                refresh_mode: None,
                file_cache: None,
                diag_missing_imports: None,
                ac_filter_model_names: None,
                auto_save_delay: None,
                add_workspace_addon_path: None,
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
            merged_entry.odoo_path = merged_entry.odoo_path.clone().or(raw_entry.odoo_path.clone());
            merged_entry.python_path = merged_entry.python_path.clone().or(raw_entry.python_path.clone());
            merged_entry.addons_paths = merged_entry.addons_paths.clone().into_iter().chain(raw_entry.addons_paths.clone().into_iter()).unique().collect();
            merged_entry.additional_stubs = merged_entry.additional_stubs.clone().into_iter().chain(raw_entry.additional_stubs.clone().into_iter()).unique().collect();
            merged_entry.refresh_mode = merged_entry.refresh_mode.clone().or(raw_entry.refresh_mode);
            merged_entry.file_cache = merged_entry.file_cache.or(raw_entry.file_cache);
            merged_entry.diag_missing_imports = merged_entry.diag_missing_imports.clone().or(raw_entry.diag_missing_imports);
            merged_entry.ac_filter_model_names = merged_entry.ac_filter_model_names.or(raw_entry.ac_filter_model_names);
            merged_entry.auto_save_delay = merged_entry.auto_save_delay.or(raw_entry.auto_save_delay);

        }
    }
    // Only infer odoo_path from workspace folders at this stage, to give priority to the user-defined one
    for (_, entry) in merged_raw_config.iter_mut() {
        if entry.odoo_path.is_none() {
            for (_name, path) in ws_folders.clone() {
                if is_odoo_path(path) {
                    if entry.odoo_path.is_some() {
                        return Err(format!(
                            "Conflict detected in 'odoo_path' for key '{}': '{}' vs '{}'\nPlease set the odoo_path in the config file.",
                            entry.name, entry.odoo_path.clone().unwrap(), path
                        ));
                    }
                    entry.odoo_path = Some(path.clone());
                }
            }
        }
    }

    // Convert the merged ConfigEntryRaw structure into ConfigEntry
    let mut final_config: ConfigNew = HashMap::new();
    for (key, raw_entry) in merged_raw_config {
        final_config.insert(
            key,
            ConfigEntry {
                odoo_path: raw_entry.odoo_path,
                addons_paths: raw_entry.addons_paths,
                python_path: raw_entry.python_path.unwrap_or(S!("python3")),
                additional_stubs: raw_entry.additional_stubs,
                refresh_mode: raw_entry.refresh_mode.unwrap_or_default(),
                file_cache: raw_entry.file_cache.unwrap_or(true),
                diag_missing_imports: raw_entry.diag_missing_imports.unwrap_or_default(),
                ac_filter_model_names: raw_entry.ac_filter_model_names.unwrap_or(true),
                auto_save_delay: raw_entry.auto_save_delay.unwrap_or(1000),
                extends: raw_entry.extends,
            },
        );
    }

    Ok(final_config)
}

impl ConfigEntry {
    pub fn new() -> Self {
        Self {
            odoo_path: None,
            addons_paths: vec![],
            python_path: S!("python3"),
            additional_stubs: vec![],
            refresh_mode: RefreshMode::default(),
            file_cache: true,
            diag_missing_imports: DiagMissingImportsMode::default(),
            ac_filter_model_names: true,
            auto_save_delay: 1000,
            extends: None,
        }
    }
}


