use std::collections::{hash_map, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, path::Path, error::Error};
use itertools::Itertools;
use serde::{Deserialize, Deserializer};

use crate::utils::{fill_validate_path, is_addon_path, is_odoo_path, is_python_path, PathSanitizer};
use crate::S;

#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
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

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
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

#[derive(Debug, Clone)]
struct Sourced<T> {
    value: T,
    sources: HashSet<String>,
}


impl<'a, T: Default> Default for Sourced<T> {
    fn default() -> Self {
        Sourced {
            value: T::default(),
            sources: HashSet::new(),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Sourced<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let value = T::deserialize(deserializer)?;
        Ok(Sourced {
            value,
            sources: HashSet::new(),
        })
    }
}

/// Merges two iterators of `Sourced<T>` into a single iterator of `Sourced<T>`.
/// By adding only unique values to the source set values but combining the source
fn merge_sourced_iters<T, I>(iter1: I, iter2: I) -> impl Iterator<Item = Sourced<T>>
where
    T: Clone + Eq + Hash,
    I: IntoIterator<Item = Sourced<T>>,
{
    iter1.into_iter()
    .chain(iter2)
    .into_group_map_by(|s| s.value.clone())
    .into_iter()
    .map(|(value, group)| Sourced {
        value,
        sources: group
            .into_iter()
            .flat_map(|s| s.sources)
            .collect::<HashSet<_>>(),
    })
}

fn merge_sourced_options<T>(opt1: Option<Sourced<T>>, opt2: Option<Sourced<T>>, profile: String, field_name: String) -> Result<Option<Sourced<T>>, String>
where
    T: Clone + Eq + Debug,
{
    match (opt1, opt2) {
        (Some(s1), Some(s2)) => {
            if s1.value != s2.value {
                return Err(format!(
                    "Conflict detected in '{profile}' for key '{field_name}': '{:?}' vs '{:?}'",
                    s1.value, s2.value
                ));
            }
            let mut merged = s1.clone();
            merged.sources.extend(s2.sources);
            Ok(Some(merged))
        }
        (Some(s), None) | (None, Some(s)) => Ok(Some(s)),
        (None, None) => Ok(None),
    }
}

// Raw structure for initial deserialization
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigEntryRaw {
    #[serde(default = "default_name")]
    name: String,

    #[serde(default)]
    extends: Option<String>, // Allowed to extend from another config

    #[serde(default)]
    odoo_path: Option<Sourced<String>>,

    #[serde(default)]
    addons_merge: Option<Sourced<MergeMethod>>,

    #[serde(default)]
    addons_paths: Vec<Sourced<String>>,

    #[serde(default)]
    python_path: Option<Sourced<String>>,

    #[serde(default)]
    additional_stubs: Vec<Sourced<String>>,

    #[serde(default)]
    additional_stubs_merge: Option<Sourced<MergeMethod>>,

    #[serde(default)]
    refresh_mode: Option<Sourced<RefreshMode>>,

    #[serde(default)]
    file_cache: Option<Sourced<bool>>,

    #[serde(default)]
    diag_missing_imports: Option<Sourced<DiagMissingImportsMode>>,

    #[serde(default)]
    ac_filter_model_names: Option<Sourced<bool>>,

    #[serde(default)]
    auto_save_delay: Option<Sourced<u64>>,

    #[serde(default)]
    add_workspace_addon_path: Option<Sourced<bool>>,
}

impl ConfigEntryRaw {
    pub fn new() -> Self {
        Self {
            name: default_name(),
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
        }
    }
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
        }
    }
}

pub type ConfigNew = HashMap<String, ConfigEntry>;


fn default_name() -> String {
    "root".to_string()
}

fn read_config_from_file<P: AsRef<Path>>(ws_folders: hash_map::Iter<String, String>, path: P) -> Result<HashMap<String, ConfigEntryRaw>, Box<dyn Error>> {
    let path = path.as_ref();
    let config_dir = path.parent().unwrap_or(Path::new("."));
    let contents = fs::read_to_string(path)?;
    let raw: ConfigFile = toml::from_str(&contents)?;

    let config = raw.config.into_iter().map(|mut entry| {
        // odoo_path
        entry.odoo_path = entry.odoo_path
            .map(|p| fill_validate_path(ws_folders.clone(), &p.value, is_odoo_path).unwrap_or(p.value.clone()))
            .and_then(|p| std::fs::canonicalize(config_dir.join(p)).ok())
            .map(|p| p.sanitize())
            .filter(|p| is_odoo_path(p))
            .map(|op| Sourced { value: op, sources: HashSet::from([path.sanitize()])});

        // addons_paths
        entry.addons_paths = entry.addons_paths.into_iter()
            .map(|sourced| fill_validate_path(ws_folders.clone(), &sourced.value, is_addon_path).unwrap_or(sourced.value.clone()))
            .flat_map(|p| std::fs::canonicalize(config_dir.join(&p)).ok())
            .map(|p| p.sanitize())
            .filter(|p| is_addon_path(p))
            .unique()
            .map(|valid| Sourced { value: valid, sources: HashSet::from([path.sanitize()])})
            .collect();

        // additional_stubs
        entry.additional_stubs.iter_mut()
            .for_each(|sourced| {
                sourced.sources.insert(path.sanitize());
            });

        // python_path
        entry.python_path = entry.python_path
            .map(|p| fill_validate_path(ws_folders.clone(), &p.value, is_python_path).unwrap_or(p.value))
            .and_then(|p| std::fs::canonicalize(config_dir.join(p)).ok())
            .map(|p| p.sanitize())
            .filter(|p| is_python_path(p))
            .map(|op| Sourced { value: op, sources: HashSet::from([path.sanitize()])});

        // Add initial source to all fields
        entry.addons_merge.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.additional_stubs_merge.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.refresh_mode.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.file_cache.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.diag_missing_imports.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.ac_filter_model_names.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.auto_save_delay.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.add_workspace_addon_path.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));

        (entry.name.clone(), entry)
    }).collect();

    Ok(config)
}

fn apply_extends(config: &mut HashMap<String, ConfigEntryRaw>){
    let keys: Vec<String> = config.keys().cloned().collect();
    for key in keys {
        if let Some(entry) = config.get(&key).cloned() {
            if let Some(extends_key) = entry.extends.clone() {
                if let Some(parent_entry) = config.get(&extends_key).cloned() {
                    let merged_entry = ConfigEntryRaw {
                        odoo_path: entry.odoo_path.as_ref().or(parent_entry.odoo_path.as_ref()).cloned(),
                        python_path: entry.python_path.or(parent_entry.python_path),
                        addons_paths: match entry.addons_merge.clone().unwrap_or_default().value {
                            MergeMethod::Merge => parent_entry.addons_paths.clone().into_iter()
                              .chain(entry.addons_paths.clone())
                              .unique_by(|v| v.value.clone())
                              .collect(),
                            MergeMethod::Override => entry.addons_paths,
                        },
                        additional_stubs: match entry.additional_stubs_merge.clone().unwrap_or_default().value {
                            MergeMethod::Merge => parent_entry.additional_stubs.clone().into_iter()
                              .chain(entry.additional_stubs.clone())
                              .unique_by(|v| v.value.clone())
                              .collect(),
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
}

fn merge_configs(
    child: &HashMap<String, ConfigEntryRaw>,
    parent: &HashMap<String, ConfigEntryRaw>,
) -> HashMap<String, ConfigEntryRaw> {
    let mut merged = HashMap::new();

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
                let addons_paths = match child.addons_merge.clone().unwrap_or_default().value {
                    MergeMethod::Merge => merge_sourced_iters(parent.addons_paths.clone(), child.addons_paths.clone()).collect(),
                    MergeMethod::Override => child.addons_paths.clone(),
                };
                let additional_stubs = match child.additional_stubs_merge.clone().unwrap_or_default().value {
                    MergeMethod::Merge => merge_sourced_iters(parent.additional_stubs.clone(), child.additional_stubs.clone()).collect(),
                    MergeMethod::Override => child.additional_stubs.clone(),
                };
                let refresh_mode = child.refresh_mode.clone().or(parent.refresh_mode.clone());
                let file_cache = child.file_cache.clone().or(parent.file_cache.clone());
                let diag_missing_imports = child.diag_missing_imports.clone().or(parent.diag_missing_imports.clone());
                let ac_filter_model_names = child.ac_filter_model_names.clone().or(parent.ac_filter_model_names.clone());
                let addons_merge = child.addons_merge.clone().or(parent.addons_merge.clone());
                let additional_stubs_merge = child.additional_stubs_merge.clone().or(parent.additional_stubs_merge.clone());
                let extends = child.extends.clone().or(parent.extends.clone());
                let auto_save_delay = child.auto_save_delay.clone().or(parent.auto_save_delay.clone());
                let add_workspace_addon_path = child.add_workspace_addon_path.clone().or(parent.add_workspace_addon_path.clone());

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


pub fn load_merged_config_upward(ws_folders: hash_map::Iter<String, String>, workspace_name: &String, workspace_path: &String) -> Result<HashMap<String, ConfigEntryRaw>, Box<dyn Error>> {
    let mut current_dir = PathBuf::from(workspace_path);
    let mut visited_dirs = HashSet::new();
    let mut merged_config: HashMap<String, ConfigEntryRaw> = HashMap::new();

    loop {
        if !visited_dirs.insert(current_dir.clone()) {
            break;
        }

        let config_path = current_dir.join("odools.toml");
        if config_path.exists() && config_path.is_file() {
            let current_config = read_config_from_file(ws_folders.clone(), &config_path)?;
            merged_config = merge_configs(&merged_config, &current_config);
            apply_extends(&mut merged_config);
        }

        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    for (_, entry) in merged_config.iter_mut() {
        if (matches!(entry.add_workspace_addon_path.as_ref().map(|a| a.value), Some(true)) || entry.addons_paths.is_empty()) && is_addon_path(workspace_path) {
            entry.addons_paths.push(Sourced { value: workspace_path.clone(), sources: HashSet::from([S!(format!("$workspaceFolder:{workspace_name}"))]) });
        }
    }

    Ok(merged_config)
}

pub fn merge_all_workspaces(
    workspace_configs: Vec<HashMap<String, ConfigEntryRaw>>,
    ws_folders: hash_map::Iter<String, String>
) -> Result<ConfigNew, String> {
    let mut merged_raw_config: HashMap<String, ConfigEntryRaw> = HashMap::new();

    for workspace_config in workspace_configs {
        for (key, raw_entry) in workspace_config {
            let merged_entry = merged_raw_config.entry(key.clone()).or_insert_with(ConfigEntryRaw::new);

            // Merge fields
            merged_entry.odoo_path = merge_sourced_options(
                merged_entry.odoo_path.clone(),
                raw_entry.odoo_path.clone(),
                key.clone(),
                "odoo_path".to_string(),
            )?;
            merged_entry.python_path = merge_sourced_options(
                merged_entry.python_path.clone(),
                raw_entry.python_path.clone(),
                key.clone(),
                "python_path".to_string(),
            )?;
            merged_entry.addons_paths = merge_sourced_iters(merged_entry.addons_paths.clone(), raw_entry.addons_paths.clone()).collect();
            merged_entry.additional_stubs = merge_sourced_iters(merged_entry.additional_stubs.clone(), raw_entry.additional_stubs.clone()).collect();
            merged_entry.refresh_mode = merge_sourced_options(
                merged_entry.refresh_mode.clone(),
                raw_entry.refresh_mode.clone(),
                key.clone(),
                "refresh_mode".to_string(),
            )?;
            merged_entry.file_cache = merge_sourced_options(
                merged_entry.file_cache.clone(),
                raw_entry.file_cache.clone(),
                key.clone(),
                "file_cache".to_string(),
            )?;
            merged_entry.diag_missing_imports = merge_sourced_options(
                merged_entry.diag_missing_imports.clone(),
                raw_entry.diag_missing_imports.clone(),
                key.clone(),
                "diag_missing_imports".to_string(),
            )?;
            merged_entry.ac_filter_model_names = merge_sourced_options(
                merged_entry.ac_filter_model_names.clone(),
                raw_entry.ac_filter_model_names.clone(),
                key.clone(),
                "ac_filter_model_names".to_string(),
            )?;
            merged_entry.auto_save_delay = merge_sourced_options(
                merged_entry.auto_save_delay.clone(),
                raw_entry.auto_save_delay.clone(),
                key.clone(),
                "auto_save_delay".to_string(),
            )?;
        }
    }
    // Only infer odoo_path from workspace folders at this stage, to give priority to the user-defined one
    for (_, entry) in merged_raw_config.iter_mut() {
        if entry.odoo_path.is_none() {
            for (name, path) in ws_folders.clone() {
                if is_odoo_path(path) {
                    if entry.odoo_path.is_some() {
                        return Err(
                            S!("More than one workspace folder is a valid odoo_path\nPlease set the odoo_path in the config file.")
                        );
                    }
                    entry.odoo_path = Some(Sourced { value: path.clone(), sources: HashSet::from([S!(format!("$workspaceFolder:{name}"))]) });
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
                odoo_path: raw_entry.odoo_path.map(|op| op.value),
                addons_paths: raw_entry.addons_paths.into_iter().map(|op| op.value).collect(),
                python_path: raw_entry.python_path.map(|op| op.value).unwrap_or(S!("python3")),
                additional_stubs: raw_entry.additional_stubs.into_iter().map(|op| op.value).collect(),
                refresh_mode: raw_entry.refresh_mode.map(|op| op.value).unwrap_or_default(),
                file_cache: raw_entry.file_cache.map(|op| op.value).unwrap_or(true),
                diag_missing_imports: raw_entry.diag_missing_imports.map(|op| op.value).unwrap_or_default(),
                ac_filter_model_names: raw_entry.ac_filter_model_names.map(|op| op.value).unwrap_or(true),
                auto_save_delay: raw_entry.auto_save_delay.map(|op| op.value).unwrap_or(1000),
            },
        );
    }

    Ok(final_config)
}

