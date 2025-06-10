use std::collections::{hash_map, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, path::Path, error::Error};
use itertools::Itertools;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::utils::{fill_validate_path, is_addon_path, is_odoo_path, is_python_path, PathSanitizer};
use crate::S;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
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


#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
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

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub config: Vec<ConfigEntryRaw>,
}

impl ConfigFile {
    pub fn new() -> Self {
        ConfigFile {
            config: vec![],
        }
    }

    pub fn to_html_string(&self) -> String {
        fn render_source(source: &str) -> String {
            if source.starts_with('/') || source.chars().nth(1) == Some(':') {
                // Windows or Unix path
                format!("<a href=\"file:///{}\">{}</a>", source.replace("\\", "/"), source)
            } else {
                source.to_string()
            }
        }

        fn is_sourced_field(val: &serde_json::Value) -> bool {
            val.get("value").is_some() && val.get("sources").is_some() && val.get("sources").unwrap().is_array()
        }

        fn render_field(key: &str, value: &serde_json::Value) -> String {
            let mut rows = String::new();
            if is_sourced_field(value) {
                let val = &value["value"];
                let sources = value["sources"].as_array().unwrap();
                let rendered_src = sources.iter()
                    .filter_map(|s| s.as_str())
                    .map(render_source)
                    .collect::<Vec<_>>()
                    .join(", ");
                if val.is_object() && !val.is_null() {
                    // Nested object
                    rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = {{</div><div class=\"toml-right\"></div></div>\n", key));
                    for (k, v) in val.as_object().unwrap() {
                        for line in render_field(k, v).lines() {
                            rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {}</div></div>\n", line));
                        }
                    }
                    rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">}</div><div class=\"toml-right\"></div></div>\n");
                } else if val.is_array() {
                    // Array of values
                    rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = [</div><div class=\"toml-right\"></div></div>\n", key));
                    for item in val.as_array().unwrap() {
                        if is_sourced_field(item) {
                            let item_val = &item["value"];
                            let item_sources = item["sources"].as_array().unwrap();
                            let item_rendered_src = item_sources.iter()
                                .filter_map(|s| s.as_str())
                                .map(render_source)
                                .collect::<Vec<_>>()
                                .join(", ");
                            rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {},</div><div class=\"toml-right\">{}</div></div>\n", item_val, item_rendered_src));
                        } else {
                            rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {},</div><div class=\"toml-right\">{}</div></div>\n", item, rendered_src));
                        }
                    }
                    rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">]</div><div class=\"toml-right\"></div></div>\n");
                } else {
                    // Single value
                    rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = {}</div><div class=\"toml-right\">{}</div></div>\n", key, val, rendered_src));
                }
            } else if value.is_array() {
                // Array of Sourced or primitive values
                rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = [</div><div class=\"toml-right\"></div></div>\n", key));
                for item in value.as_array().unwrap() {
                    if is_sourced_field(item) {
                        let item_val = &item["value"];
                        let item_sources = item["sources"].as_array().unwrap();
                        let item_rendered_src = item_sources.iter()
                            .filter_map(|s| s.as_str())
                            .map(render_source)
                            .collect::<Vec<_>>()
                            .join(", ");
                        rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {},</div><div class=\"toml-right\">{}</div></div>\n", item_val, item_rendered_src));
                    } else {
                        rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {},</div><div class=\"toml-right\"></div></div>\n", item));
                    }
                }
                rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">]</div><div class=\"toml-right\"></div></div>\n");
            } else if value.is_object() && !value.is_null() {
                // Nested object
                rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = {{</div><div class=\"toml-right\"></div></div>\n", key));
                for (k, v) in value.as_object().unwrap() {
                    for line in render_field(k, v).lines() {
                        rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {}</div></div>\n", line));
                    }
                }
                rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">}</div><div class=\"toml-right\"></div></div>\n");
            } else {
                // Primitive value
                rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = {}</div><div class=\"toml-right\"></div></div>\n", key, value));
            }
            rows
        }

        let mut html = String::from(
            r#"<style>
  .toml-table {
    font-family: monospace;
    width: 100%;
    border-spacing: 0;
  }
  .toml-row {
    display: flex;
    justify-content: space-between;
    padding: 2px 0;
  }
  .toml-line-break {
    border-bottom: 1px dotted #ccc;
  }
  .toml-left {
    white-space: pre;
  }
  .toml-right {
    white-space: pre;
    color: #888;
    font-size: 0.9em;
  }
</style>
<div class="toml-table">
"#,
        );
        let entry_htmls: Vec<String> = self.config.iter().map(|entry| {
            let entry_val = serde_json::to_value(entry).unwrap_or(serde_json::Value::Null);
            let mut entry_html = String::new();
            entry_html.push_str("<div class=\"toml-row\"><div class=\"toml-left\"><b>[[config]]</b></div><div class=\"toml-right\"></div></div>\n");
            if let serde_json::Value::Object(map) = entry_val {
                let order = [
                    "name", "extends", "odoo_path", "addons_paths", "addons_merge",
                    "python_path", "additional_stubs", "additional_stubs_merge",
                    "refresh_mode", "file_cache", "diag_missing_imports",
                    "ac_filter_model_names", "auto_save_delay", "add_workspace_addon_path",
                ];
                for key in order {
                    if let Some(val) = map.get(key) {
                        entry_html.push_str(&render_field(key, val));
                    }
                }
            }
            entry_html
        }).collect();

        html.push_str(&entry_htmls.join("<div class=\"toml-line-break\"></div>\n"));
        html.push_str("</div>\n");
        html
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Sourced<T> {
    value: T,
    sources: HashSet<String>,
}

impl<T> Sourced<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn sources(&self) -> &HashSet<String> {
        &self.sources
    }
}

impl<'a, T: Default> Default for Sourced<T> {
    fn default() -> Self {
        Sourced {
            value: T::default(),
            sources: HashSet::from([S!("$default")]),
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

pub fn serialize_option_as_default<T, S>(opt: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize + Default,
    S: Serializer,
{
    match opt {
        Some(val) => val.serialize(serializer),
        None => T::default().serialize(serializer),
    }
}
// Raw structure for initial deserialization
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigEntryRaw {
    #[serde(default = "default_name")]
    pub name: String,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    extends: Option<String>, // Allowed to extend from another config

    #[serde(default, serialize_with = "serialize_option_as_default")]
    odoo_path: Option<Sourced<String>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    addons_merge: Option<Sourced<MergeMethod>>,

    #[serde(default)]
    addons_paths: Vec<Sourced<String>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    python_path: Option<Sourced<String>>,

    #[serde(default)]
    additional_stubs: Vec<Sourced<String>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    additional_stubs_merge: Option<Sourced<MergeMethod>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    refresh_mode: Option<Sourced<RefreshMode>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    file_cache: Option<Sourced<bool>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    diag_missing_imports: Option<Sourced<DiagMissingImportsMode>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    ac_filter_model_names: Option<Sourced<bool>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    auto_save_delay: Option<Sourced<u64>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
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

    pub fn python_path_sourced(&self) -> Option<&Sourced<String>> {
        self.python_path.as_ref()
    }
    pub fn file_cache_sourced(&self) -> Option<&Sourced<bool>> {
        self.file_cache.as_ref()
    }
    pub fn auto_save_delay_sourced(&self) -> Option<&Sourced<u64>> {
        self.auto_save_delay.as_ref()
    }
    pub fn addons_paths_sourced(&self) -> &Vec<Sourced<String>> {
        &self.addons_paths
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

impl Default for ConfigEntry {
    fn default() -> Self {
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

impl ConfigEntry {
    pub fn new() -> Self {
        Self::default()
    }
}

pub type ConfigNew = HashMap<String, ConfigEntry>;


fn default_name() -> String {
    "root".to_string()
}

fn read_config_from_file<P: AsRef<Path>>(ws_folders: &HashMap<String, String>, path: P, workspace_name: &String) -> Result<HashMap<String, ConfigEntryRaw>, String> {
    let path = path.as_ref();
    let config_dir = path.parent().unwrap_or(Path::new("."));
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let raw = toml::from_str::<ConfigFile>(&contents).map_err(|err| err.to_string())?;

    fn fill_or_canonicalize<F>(sourced_path: Sourced<String>, ws_folders: &HashMap<String, String>, workspace_name: &String, config_dir: &Path, predicate: F) -> Option<PathBuf>
    where
    F: Fn(&String) -> bool,
    {
        // If it is a valid pattern, in $PATH, a valid alias or a valid absolute path: (no need to canonicalize like python3 for example)
        fill_validate_path(ws_folders, workspace_name, &sourced_path.value, predicate).map(PathBuf::from)
        // If it is relative path it should work here
        .or(std::fs::canonicalize(config_dir.join(sourced_path.value)).ok())
    }

    let config = raw.config.into_iter().map(|mut entry| {
        // odoo_path
        entry.odoo_path = entry.odoo_path
            .and_then(|p| fill_or_canonicalize(p, ws_folders, workspace_name, config_dir, is_odoo_path))
            .map(|p| p.sanitize())
            .filter(|p| is_odoo_path(p))
            .map(|op| Sourced { value: op, sources: HashSet::from([path.sanitize()])});

        // addons_paths
        entry.addons_paths = entry.addons_paths.into_iter()
            .flat_map(|p| fill_or_canonicalize(p, ws_folders, workspace_name, config_dir, is_addon_path))
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
            .and_then(|p| fill_or_canonicalize(p, ws_folders, workspace_name, config_dir, is_python_path))
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

fn apply_extends(config: &mut HashMap<String, ConfigEntryRaw>) -> Result<(), String> {
/*
    each profile has a parent, Option<String>
    each profile can have multiple children, Vec<String>

    So we have to construct an N-tree structure
    where each node is a profile, and each edge is an extends relationship.
    We can then traverse the tree and merge the profiles from the top after we have a topo sort for each component.

    This way we can also detect circular dependencies.
 */
    struct Node {
        parent: Option<String>,
        children: HashSet<String>,
    }
    let keys: Vec<String> = config.keys().cloned().collect();
    let mut nodes: HashMap<String, Node> = keys.iter().map(|key|
        (key.clone(), Node {
            parent: None,
            children: HashSet::new(),
        })
    ).collect();
    let edges: Vec<(String, String)> = keys.iter().filter_map(|key| {
        match config.get(key).and_then(|entry| entry.extends.clone()) {
            Some(extends_key) => Some((key.clone(), extends_key)),
            None => None,
        }
    }).collect();
    for (child, parent) in edges {
        nodes.get_mut(&child).unwrap().parent = Some(parent.clone());
        let Some(node) = nodes.get_mut(&parent) else {
            return Err(S!(format!("Profile '{}' extends non-existing profile '{}'", child, parent)));
        };
        node.children.insert(child);
    }

    let mut ordered_nodes= vec![];
    let mut visited = HashSet::new();
    for parent in nodes.iter().filter(|(_, n)| n.parent.is_none()){
        let mut stack = vec![parent.0.clone()];
        while let Some(current) = stack.pop().map(|value| {ordered_nodes.push(value.clone()); value}) {
            if visited.contains(&current) {
                return Err(S!("Circular dependency detected in profile extensions!"));
            }
            visited.insert(current.clone());
            if let Some(node) = nodes.get(&current) {
                for child in &node.children {
                    stack.push(child.clone());
                }
            }
        }
    }
    if visited.len() != nodes.len() {
        return Err(S!("Circular dependency detected in profile extensions!"))
    }

    for key in ordered_nodes.iter(){
        if let Some(entry) = config.get(key).cloned() {
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
                    config.insert(key.clone(), merged_entry);
                }
            }
        }
    }
    Ok(())
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


fn load_merged_config_upward(ws_folders: &HashMap<String, String>, workspace_name: &String, workspace_path: &String) -> Result<HashMap<String, ConfigEntryRaw>, String> {
    let mut current_dir = PathBuf::from(workspace_path);
    let mut visited_dirs = HashSet::new();
    let mut merged_config: HashMap<String, ConfigEntryRaw> = HashMap::new();
    merged_config.insert("root".to_string(), ConfigEntryRaw::new());

    loop {
        if !visited_dirs.insert(current_dir.clone()) {
            break;
        }

        let config_path = current_dir.join("odools.toml");
        if config_path.exists() && config_path.is_file() {
            let current_config = read_config_from_file(ws_folders, &config_path, workspace_name)?;
            merged_config = merge_configs(&merged_config, &current_config);
        }
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }
    apply_extends(&mut merged_config)?;

    for (_, entry) in merged_config.iter_mut() {
        if (matches!(entry.add_workspace_addon_path.as_ref().map(|a| a.value), Some(true)) || entry.addons_paths.is_empty()) && is_addon_path(workspace_path) {
            entry.addons_paths.push(Sourced { value: workspace_path.clone(), sources: HashSet::from([S!(format!("$workspaceFolder:{workspace_name}"))]) });
        }
    }

    Ok(merged_config)
}

fn merge_all_workspaces(
    workspace_configs: Vec<HashMap<String, ConfigEntryRaw>>,
    ws_folders: &HashMap<String, String>
) -> Result<(ConfigNew, ConfigFile), String> {
    let mut merged_raw_config: HashMap<String, ConfigEntryRaw> = HashMap::new();

    for workspace_config in workspace_configs {
        for (key, raw_entry) in workspace_config {
            let merged_entry = merged_raw_config.entry(key.clone()).or_insert_with(ConfigEntryRaw::new);
            merged_entry.name = key.clone();

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
            for (name, path) in ws_folders.iter() {
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

    let config_file = ConfigFile { config: merged_raw_config.values().cloned().collect::<Vec<_>>()};

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

    Ok((final_config, config_file))
}

pub fn get_configuration(ws_folders: &HashMap<String, String>)  -> Result<(ConfigNew, ConfigFile), String> {
    let ws_confs: Result<Vec<_>, _> = ws_folders.iter().map(|ws_f| load_merged_config_upward(ws_folders, ws_f.0, ws_f.1)).collect();
    merge_all_workspaces(ws_confs?, ws_folders)
}
