use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, path::Path};
use itertools::Itertools;
use glob::Pattern;
use regex::Regex;
use ruff_python_ast::{Expr, Mod};
use ruff_python_parser::{Mode, ParseOptions};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use tracing::error;

use crate::constants::{CONFIG_WIKI_URL};
use crate::core::diagnostics::{DiagnosticCode, DiagnosticSetting, SchemaDiagnosticCodes};
use crate::utils::{fill_validate_path, get_python_command, has_template, is_addon_path, is_odoo_path, is_python_path, PathSanitizer};
use crate::S;


static VERSION_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"^(\D+~)?\d+\.\d+$"#).unwrap()
});

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone, JsonSchema)]
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


#[derive(Debug, Deserialize, Serialize, Clone, Copy, JsonSchema)]
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

#[derive(Debug, Deserialize, Serialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticFilterPathType {
    In,
    NotIn
}

impl Default for DiagnosticFilterPathType {
    fn default() -> Self {
        DiagnosticFilterPathType::In
    }
}

#[derive(Debug, Clone, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct DiagnosticFilter {
    #[schemars(default, schema_with = "regex_vec_schema")]
    pub paths: Vec<Pattern>,
    #[schemars(default, schema_with = "regex_vec_schema")]
    pub codes: Vec<Regex>,
    #[schemars(default)]
    pub types: Vec<DiagnosticSetting>,
    #[schemars(default)]
    pub path_type: DiagnosticFilterPathType,
}

/// Serialize the schema as Vec<String> and adds the default
fn regex_vec_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema = <Vec<String>>::json_schema(generator);
    schema.insert("default".into(), serde_json::json!([]));
    schema
}

impl<'de> serde::Deserialize<'de> for DiagnosticFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Helper {
            paths: Vec<String>,
            #[serde(default)]
            codes: Vec<String>,
            #[serde(default)]
            types: Vec<DiagnosticSetting>,
            #[serde(default)]
            path_type: DiagnosticFilterPathType,
        }
        let helper = Helper::deserialize(deserializer)?;
        let mut paths = vec![];
        for path in helper.paths.iter() {
            let sanitized_path = PathBuf::from(path).sanitize();
            let path_pattern = Pattern::new(&sanitized_path).map_err(serde::de::Error::custom)?;
            paths.push(path_pattern);
        }
        let mut code_regexes = Vec::with_capacity(helper.codes.len());
        for code in &helper.codes {
            let regex = Regex::new(code).map_err(serde::de::Error::custom)?;
            code_regexes.push(regex);
        }
        for t in &helper.types {
            if let DiagnosticSetting::Disabled = t {
                return Err(serde::de::Error::custom("DiagnosticFilter.types cannot contain 'Disabled'"));
            }
        }
        Ok(DiagnosticFilter {
            paths: paths,
            codes: code_regexes,
            types: helper.types,
            path_type: helper.path_type,
        })
    }
}

impl Serialize for DiagnosticFilter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("DiagnosticFilter", 3)?;
        let paths: Vec<String> = self.paths.iter().map(|p| p.as_str().to_string()).collect();
        s.serialize_field("paths", &paths)?;
        let codes: Vec<String> = self.codes.iter().map(|r| r.as_str().to_string()).collect();
        s.serialize_field("codes", &codes)?;
        s.serialize_field("types", &self.types)?;
        s.serialize_field("path_type", &self.path_type)?;
        s.end()
    }
}

#[derive(Debug, Deserialize, Clone, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
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

        fn sourced_info(val: &serde_json::Value) -> Option<String> {
            val.get("info")
                .and_then(|info| info.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| format!("<span class=\"toml-info\">{}</span>", s))
        }

        fn render_field(key: &str, value: &serde_json::Value, ident: usize) -> String {
            let mut rows = String::new();
            if is_sourced_field(value) {
                let val = &value["value"];
                let sources = value["sources"].as_array().unwrap();
                let rendered_src = sources.iter()
                    .filter_map(|s| s.as_str())
                    .map(render_source)
                    .collect::<Vec<_>>()
                    .join(", ");
                let info_html = sourced_info(value).unwrap_or_default();
                if val.is_object() && !val.is_null() {
                    // Nested object
                    rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = {{</div><div class=\"toml-right\">{}</div></div>\n", key, info_html));
                    for (k, v) in val.as_object().unwrap() {
                        for line in render_field(k, v, ident + 1).lines() {
                            rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">  {}</div></div>\n", line));
                        }
                    }
                    rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">}</div><div class=\"toml-right\"></div></div>\n");
                } else if val.is_array() {
                    // Array of values
                    rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = [</div><div class=\"toml-right\">{}</div></div>\n", key, info_html));
                    for item in val.as_array().unwrap() {
                        if is_sourced_field(item) {
                            let item_val = &item["value"];
                            let item_sources = item["sources"].as_array().unwrap();
                            let item_rendered_src = item_sources.iter()
                                .filter_map(|s| s.as_str())
                                .map(render_source)
                                .collect::<Vec<_>>()
                                .join(", ");
                            let item_info_html = sourced_info(item).unwrap_or_default();
                            rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{}{},</div><div class=\"toml-right\">{}{}</div></div>\n", " ".repeat((ident + 1) * 2), item_val, item_rendered_src, item_info_html));
                        } else {
                            rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{}{},</div><div class=\"toml-right\">{}</div></div>\n", " ".repeat((ident + 1) * 2), item, rendered_src));
                        }
                    }
                    rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">]</div><div class=\"toml-right\"></div></div>\n");
                } else {
                    // Single value
                    rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{}{} = {}</div><div class=\"toml-right\">{}{}</div></div>\n", " ".repeat(ident * 2), key, val, rendered_src, info_html));
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
                        let item_info_html = sourced_info(item).unwrap_or_default();
                        rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{}{},</div><div class=\"toml-right\">{}{}</div></div>\n", " ".repeat((ident + 1) * 2), item_val, item_rendered_src, item_info_html));
                    } else {
                        rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{}{},</div><div class=\"toml-right\"></div></div>\n", " ".repeat((ident + 1) * 2), item));
                    }
                }
                rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">]</div><div class=\"toml-right\"></div></div>\n");
            } else if value.is_object() && !value.is_null() {
                // Nested object
                rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{} = {{</div><div class=\"toml-right\"></div></div>\n", key));
                for (k, v) in value.as_object().unwrap() {
                    for line in render_field(k, v, ident + 1).lines() {
                        rows.push_str(line);
                    }
                }
                rows.push_str("<div class=\"toml-row\"><div class=\"toml-left\">}</div><div class=\"toml-right\"></div></div>\n");
            } else {
                // Primitive value
                rows.push_str(&format!("<div class=\"toml-row\"><div class=\"toml-left\">{}{} = {}</div><div class=\"toml-right\"></div></div>\n", " ".repeat(ident * 2), key, value));
            }
            rows
        }

        let mut html = String::from(
            r#"<style>
  body {
    background: #1F1F1F;
    color: #d6d6d6ff;
  }
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
    .toml-info {
        color: #c00;
        font-size: 0.9em;
        margin-left: 10px;
        font-style: italic;
    }
  .config-wiki-link {
    margin-bottom: 10px;
    display: block;
    font-family: sans-serif;
    font-size: 1em;
    color: #2d5fa4;
    text-decoration: none;
    font-weight: bold;
  }
  a:visited {
    color: #2d5fa4 !important;  /* visited links same as normal */
  }
</style>
<a class="config-wiki-link" href=""#);
        html.push_str(CONFIG_WIKI_URL);
        html.push_str("\" target=\"_blank\" rel=\"noopener\">Configuration file documentation &rarr;</a>\n");
        html.push_str("<div class=\"toml-table\">\n");
        let entry_htmls: Vec<String> = self.config.iter().map(|entry| {
            let entry_val = serde_json::to_value(entry).unwrap_or(serde_json::Value::Null);
            let mut entry_html = String::new();
            entry_html.push_str("<div class=\"toml-row\"><div class=\"toml-left\"><b>[[config]]</b></div><div class=\"toml-right\"></div></div>\n");
            if let serde_json::Value::Object(map) = entry_val {
                let order = [
                    "name", "extends", "odoo_path", "abstract", "addons_paths", "addons_merge",
                    "python_path", "stdlib", "additional_stubs", "additional_stubs_merge",
                    "refresh_mode", "file_cache", "diag_missing_imports",
                    "ac_filter_model_names", "auto_refresh_delay",
                    "diagnostic_settings", "diagnostic_filters", "no_typeshed_stubs"
                ];
                for key in order {
                    if let Some(val) = map.get(key) {
                        entry_html.push_str(&render_field(key, val, 0));
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Sourced<T> {
    value: T,
    sources: HashSet<String>,
    info: String,
}

impl<T> Sourced<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn sources(&self) -> &HashSet<String> {
        &self.sources
    }
}

impl<T: Default> Default for Sourced<T> {
    fn default() -> Self {
        Sourced {
            value: T::default(),
            sources: HashSet::from([S!("$default")]),
            info: String::new(),
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
            info: String::new(),
        })
    }
}

/// Merges two iterators of `Sourced<T>` into a single iterator of `Sourced<T>`.
/// By adding only unique values to the source set values but combining the source
fn merge_sourced_iters<T, I>(iter1: I, iter2: I) -> impl Iterator<Item = Sourced<T>>
where
    T: Clone + Eq + Hash + Default,
    I: IntoIterator<Item = Sourced<T>>,
{
    group_sourced_iters(
        iter1.into_iter()
        .chain(iter2)
    )
}

/// Groups `Sourced<T>` items by their value, merging their sources into a single `Sourced<T>`.
fn group_sourced_iters<T, I>(iter: I) -> impl Iterator<Item = Sourced<T>>
where
    T: Clone + Eq + Hash + Default,
    I: IntoIterator<Item = Sourced<T>>,
{
    iter.into_iter()
    .into_group_map_by(|s| s.value.clone())
    .into_iter()
    .map(|(value, group)| Sourced {
        value,
        sources: group
            .into_iter()
            .flat_map(|s| s.sources)
            .collect::<HashSet<_>>(),
        ..Default::default()
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

pub fn serialize_python_path<T, S>(opt: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize + Default,
    S: Serializer,
{
    match opt {
        Some(val) => val.serialize(serializer),
        None => (Sourced { value: S!(get_python_command().unwrap_or_default()), ..Default::default() }).serialize(serializer),
    }
}

pub fn serialize_file_cache<T, S>(opt: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize + Default,
    S: Serializer,
{
    match opt {
        Some(val) => val.serialize(serializer),
        None => (Sourced { value: true, ..Default::default() }).serialize(serializer),
    }
}

pub fn serialize_ac_filter_model_names<T, S>(opt: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize + Default,
    S: Serializer,
{
    match opt {
        Some(val) => val.serialize(serializer),
        None => (Sourced { value: true, ..Default::default() }).serialize(serializer),
    }
}

pub fn serialize_auto_refresh_delay<T, S>(opt: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize + Default,
    S: Serializer,
{
    match opt {
        Some(val) => val.serialize(serializer),
        None => (Sourced { value: 1000, ..Default::default() }).serialize(serializer),
    }
}

fn parse_manifest_version(contents: String) -> Option<String> {
    let parsed = ruff_python_parser::parse_unchecked(contents.as_str(), ParseOptions::from(Mode::Module));
    if !parsed.errors().is_empty() {
        return None;
    }
    let Mod::Module(module) = parsed.into_syntax() else {
        return None;
    };
    if module.body.len() != 1 {
        return None; // We expect only one statement in the manifest
    }
    let Some(dict_expr) = module.body.first()
    .and_then(|stmt| stmt.as_expr_stmt())
    .and_then(|expr| expr.value.as_dict_expr()) else {
        return None; // We expect a single expression that is a dictionary
    };
    for item in dict_expr.items.iter() {
        if !matches!(item.key.as_ref(), Some(Expr::StringLiteral(expr)) if expr.value.to_str() == "version") {
            continue;
        }
        if let Some(value_expr) = item.value.as_string_literal_expr() {
            return Some(value_expr.value.to_string());
        }
    }
    None
}

fn process_version(var: Sourced<String>, ws_folders: &HashMap<String, String>, workspace_name: Option<&String>) -> Sourced<String> {
    let Some(config_path) = var.sources.iter().next().map(PathBuf::from) else {
        unreachable!("Expected at least one source for sourced_path: {:?}", var);
    };
    let config_dir = config_path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    match fill_validate_path(ws_folders, workspace_name, var.value(), |p| PathBuf::from(p).exists(), HashMap::new(), &config_dir) {
        Ok(filled_path) => {
            let var_pb = PathBuf::from(&filled_path);
            if var_pb.is_file() {
                // If it is a file, we can return the file name
                let Some(file_name) = var_pb.file_name() else {
                    unreachable!("Expected a file name for path {:?}", &var_pb);
                };
                let f_name = file_name.to_string_lossy();
                if f_name == "__manifest__.py" {
                    // If it is a manifest file, we can return the version from it
                    if let Some((Some(major), Some(minor))) = fs::read_to_string(&var_pb).ok()
                    .and_then(|contents| parse_manifest_version(contents.clone()))
                    .map(|version| {
                        let mut parts = version.trim_matches('"').split('.');
                        (parts.next().map(|s| s.to_string()), parts.next().map(|s| s.to_string()))
                    }) {
                        return Sourced { value: S!(format!("{}.{}", major, minor)), sources: var.sources.clone(), ..Default::default() };
                    }
                    return var;
                }
            }
            let Ok(var_pb) = var_pb.canonicalize() else {
                unreachable!("Failed to canonicalize path {:?}", &filled_path);
            };
            let Some(suffix) = var_pb.components().last() else {
                unreachable!("Invalid variable value {:?}", &filled_path);
            };
            Sourced { value: S!(suffix.as_os_str().to_string_lossy()), sources: var.sources.clone(), ..Default::default() }
        },
        // Not a valid path, just return the variable as is, log info
        Err(err) => {
            error!("Failed to process $version path for variable {:?}: {}", var, err);
            var
        },
    }
}

// Raw structure for initial deserialization

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[schemars(transform = transform_defaults, deny_unknown_fields)]
pub struct ConfigEntryRaw {
    #[serde(default = "default_profile_name")]
    pub name: String,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    extends: Option<String>, // Allowed to extend from another config

    #[serde(default, serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<String>")]
    odoo_path: Option<Sourced<String>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<MergeMethod>")]
    addons_merge: Option<Sourced<MergeMethod>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<Vec<String>>")]
    addons_paths: Option<Vec<Sourced<String>>>,

    #[serde(default, serialize_with = "serialize_python_path")]
    #[schemars(with = "Option<String>")]
    python_path: Option<Sourced<String>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<Vec<String>>")]
    additional_stubs: Option<Vec<Sourced<String>>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<MergeMethod>")]
    additional_stubs_merge: Option<Sourced<MergeMethod>>,

    #[serde(default, serialize_with = "serialize_file_cache")]
    #[schemars(with = "Option<bool>")]
    file_cache: Option<Sourced<bool>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<DiagMissingImportsMode>")]
    diag_missing_imports: Option<Sourced<DiagMissingImportsMode>>,

    #[serde(default, serialize_with = "serialize_ac_filter_model_names")]
    #[schemars(with = "Option<bool>")]
    ac_filter_model_names: Option<Sourced<bool>>,

    #[serde(default, serialize_with = "serialize_auto_refresh_delay")]
    #[schemars(with = "Option<u64>")]
    auto_refresh_delay: Option<Sourced<u64>>,

    #[serde(default, rename(serialize = "$version", deserialize = "$version"), serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<String>")]
    version: Option<Sourced<String>>,

    #[serde(default, rename(serialize = "$base", deserialize = "$base"), serialize_with = "serialize_option_as_default")]
    #[schemars(with = "Option<String>")]
    base: Option<Sourced<String>>,

    #[serde(default)]
    #[schemars(with = "SchemaDiagnosticCodes")]
    diagnostic_settings: HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>,

    #[serde(default)]
    #[schemars(with = "Vec<DiagnosticFilter>")]
    diagnostic_filters: Vec<Sourced<DiagnosticFilter>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    stdlib: Option<Sourced<String>>,

    #[serde(default, serialize_with = "serialize_option_as_default")]
    no_typeshed_stubs: Option<Sourced<bool>>,

    #[serde(skip_deserializing, rename(serialize = "abstract"))]
    abstract_: bool
}

fn transform_defaults(schema: &mut Schema) {
    use serde_json::Value;
    fn recurse(val: &mut Value) {
        match val {
            Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    if k == "default" {
                        if let Value::Object(def_map) = v {
                            if def_map.contains_key("info") && def_map.contains_key("sources") && def_map.contains_key("value")
                            {
                                *v = def_map.get("value").cloned().unwrap();
                            }
                        }
                    } else {
                        recurse(v);
                    }
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    recurse(v);
                }
            }
            _ => {}
        }
    }
    if let Some(value) = schema.pointer_mut("/properties") {
        recurse(value);
    }
}

impl Default for ConfigEntryRaw {
    fn default() -> Self {
        Self {
            name: default_profile_name(),
            extends: None,
            odoo_path: None,
            addons_merge: None,
            addons_paths: None,
            python_path: None,
            additional_stubs: None,
            additional_stubs_merge: None,
            file_cache: None,
            diag_missing_imports: None,
            ac_filter_model_names: None,
            auto_refresh_delay: None,
            version: None,
            base: None,
            diagnostic_settings: Default::default(),
            abstract_: false,
            diagnostic_filters: vec![],
            stdlib: None,
            no_typeshed_stubs: None,
        }
    }
}

impl ConfigEntryRaw {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn python_path_sourced(&self) -> Option<&Sourced<String>> {
        self.python_path.as_ref()
    }
    pub fn file_cache_sourced(&self) -> Option<&Sourced<bool>> {
        self.file_cache.as_ref()
    }
    pub fn auto_refresh_delay_sourced(&self) -> Option<&Sourced<u64>> {
        self.auto_refresh_delay.as_ref()
    }
    pub fn addons_paths_sourced(&self) -> &Option<Vec<Sourced<String>>> {
        &self.addons_paths
    }
    pub fn is_abstract(&self) -> bool {
        self.abstract_
    }
}


#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub name: String,
    pub odoo_path: Option<String>,
    pub addons_paths: HashSet<String>,
    pub python_path: String,
    pub additional_stubs: HashSet<String>,
    pub file_cache: bool,
    pub diag_missing_imports: DiagMissingImportsMode,
    pub ac_filter_model_names: bool,
    pub auto_refresh_delay: u64,
    pub stdlib: String,
    pub no_typeshed_stubs: bool,
    pub abstract_: bool,
    pub diagnostic_settings: HashMap<DiagnosticCode, DiagnosticSetting>,
    pub diagnostic_filters: Vec<DiagnosticFilter>,
}

impl Default for ConfigEntry {
    fn default() -> Self {
        Self {
            name: default_profile_name(),
            odoo_path: None,
            addons_paths: HashSet::new(),
            python_path: S!(get_python_command().unwrap_or_default()),
            additional_stubs: HashSet::new(),
            file_cache: true,
            diag_missing_imports: DiagMissingImportsMode::default(),
            ac_filter_model_names: true,
            auto_refresh_delay: 1000,
            stdlib: S!(""),
            no_typeshed_stubs: false,
            abstract_: false,
            diagnostic_settings: Default::default(),
            diagnostic_filters: vec![],
        }
    }
}

impl ConfigEntry {
    pub fn new() -> Self {
        Self::default()
    }
}

pub type ConfigNew = HashMap<String, ConfigEntry>;


pub fn default_profile_name() -> String {
    "default".to_string()
}

fn fill_or_canonicalize<F>(sourced_path: &Sourced<String>, ws_folders: &HashMap<String, String>, workspace_name: Option<&String>, predicate: &F, var_map: HashMap<String, String>) -> Result<Sourced<String>, String>
where
F: Fn(&String) -> bool,
{
    let Some(config_path) = sourced_path.sources.iter().next().map(PathBuf::from) else {
        unreachable!("Expected at least one source for sourced_path: {:?}", sourced_path);
    };
    let config_dir = config_path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if has_template(&sourced_path.value) {
        return fill_validate_path(ws_folders, workspace_name, &sourced_path.value, predicate, var_map, &config_dir)
        .and_then(|p| std::fs::canonicalize(PathBuf::from(p)).map_err(|e| e.to_string()))
        .map(|p| p.sanitize())
        .map(|path| Sourced { value: path, sources: sourced_path.sources.clone(), ..Default::default()});
    }
    let mut path = PathBuf::from(&sourced_path.value);
    if path.is_relative() {
        path = config_dir.join(sourced_path.value.clone());
    }
    let path = std::fs::canonicalize(path)
    .map_err(|e| e.to_string())?
    .sanitize();
    if !predicate(&path) {
        return Err(format!("Path '{}' does not satisfy the required conditions", path));
    }
    Ok(Sourced { value: path, sources: sourced_path.sources.clone(), ..Default::default() })
}

fn process_paths(
    entry: &mut ConfigEntryRaw,
    ws_folders: &HashMap<String, String>,
    workspace_name: Option<&String>,
){
    let mut var_map: HashMap<String, String> = HashMap::new();
    if let Some(v) = entry.version.clone() {
        var_map.insert(S!("version"), v.value().clone());
    }
    if let Some(b) = entry.base.clone() {
        var_map.insert(S!("base"), b.value().clone());
    }
    entry.odoo_path =  entry.odoo_path.as_ref()
        .and_then(|p| fill_or_canonicalize(p, ws_folders, workspace_name, &is_odoo_path, var_map.clone())
            .map_err(|err| error!("Failed to process odoo path for variable {:?}: {}", p, err))
            .ok()
        );

    let infer = entry.addons_paths.as_mut().map_or(true, |ps| {
        let initial_len = ps.len();
        ps.retain(|v| v.value != S!("$autoDetectAddons"));
        initial_len != ps.len() // $autoDetectAddons is found
    });
    entry.addons_paths = entry.addons_paths.as_ref().map(|paths|
        paths.iter().filter_map(|sourced| {
            fill_or_canonicalize(sourced, ws_folders, workspace_name, &is_addon_path, var_map.clone())
            .map_err(|err| error!("Failed to process addons path for variable {:?}: {}", sourced, err))
            .ok()
        }).collect()
    );
    if infer {
        if let Some((name, workspace_path)) = workspace_name.and_then(|name| ws_folders.get(name).map(|p| (name, p))) {
            let workspace_path = PathBuf::from(workspace_path).sanitize();
            if is_addon_path(&workspace_path) {
                let addon_path = Sourced { value: workspace_path.clone(), sources: HashSet::from([S!(format!("$workspaceFolder:{name}"))]), ..Default::default()};
                match entry.addons_paths {
                    Some(ref mut paths) => paths.push(addon_path),
                    None => entry.addons_paths = Some(vec![addon_path]),
                }
            }
        }
    }
    entry.python_path = entry.python_path.as_ref()
        .and_then(|p| {
            if is_python_path(&p.value) {
                Some(p.clone())
            } else {
                fill_or_canonicalize(p, ws_folders, workspace_name, &is_python_path, var_map.clone())
                .map_err(|err| error!("Failed to fill or canonicalize python path for variable {:?}: {}", p, err))
                .ok()
            }
        });
    entry.stdlib.as_mut().map(|std|{
        let maybe_value = std::fs::canonicalize(PathBuf::from(&std.value)).map(|p| p.sanitize());
        match maybe_value {
            Ok(path) => {
                std.value = path;
            }
            Err(err) => {
                std.value = S!("");
                std.info = format!("Failed to canonicalize stdlib path: {}", err);
            }
        }
    });
    entry.diagnostic_filters.iter_mut().for_each(|filter| {
        let Some(config_path) = filter.sources.iter().next().map(PathBuf::from) else {
            unreachable!("Expected at least one source for sourced_path: {:?}", filter);
        };
        let config_dir = config_path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        filter.value.paths = filter.value.paths.iter().filter_map(|pattern| {
            let pattern_string = pattern.to_string();
            let processed_pattern = fill_validate_path(ws_folders, workspace_name, &pattern_string, &|_: &String| true, var_map.clone(), &config_dir)
                .and_then(|p| Pattern::new(&p)
                .map_err(|e| e.to_string()));
            match processed_pattern {
                Ok(p) => Some(p),
                Err(err) => {
                    let message = format!("Failed to process pattern '{}': {}", pattern_string, err);
                    filter.info = filter.info.clone() + &message;
                    error!(message);
                    None
                }
            }
        }).collect();
    });
}

fn read_config_from_file<P: AsRef<Path>>(path: P) -> Result<HashMap<String, ConfigEntryRaw>, String> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let raw = toml::from_str::<ConfigFile>(&contents).map_err(|err| err.to_string())?;


    let config = raw.config.into_iter().map(|mut entry| {
        // odoo_path
        entry.odoo_path.iter_mut().for_each(|sourced| { sourced.sources.insert(path.sanitize());});

        // addons_paths
        entry.addons_paths.iter_mut().for_each(|paths| {
            paths.iter_mut().for_each(|sourced| {
                sourced.sources.insert(path.sanitize());
            });
        });

        // additional_stubs
        entry.additional_stubs.iter_mut().for_each(|stubs| {
            stubs.iter_mut().for_each(|sourced| {
                sourced.sources.insert(path.sanitize());
            });
        });

        // python_path
        entry.python_path.as_mut().map(|sourced| { sourced.sources.insert(path.sanitize());});

        // Add initial source to all fields
        entry.addons_merge.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.additional_stubs_merge.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.file_cache.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.diag_missing_imports.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.ac_filter_model_names.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.auto_refresh_delay.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.version.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.diagnostic_settings.values_mut().for_each(|sourced| {
            sourced.sources.insert(path.sanitize());
        });
        entry.diagnostic_filters.iter_mut().for_each(|filter| {
            filter.sources.insert(path.sanitize());
        });
        entry.stdlib.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));
        entry.no_typeshed_stubs.as_mut().map(|sourced| sourced.sources.insert(path.sanitize()));

        (entry.name.clone(), entry)
    }).collect();

    Ok(config)
}
/// Merge two HashMap<DiagnosticCode, Sourced<DiagnosticSetting>> for diagnostic_settings, combining sources for identical values.
fn merge_sourced_diagnostic_setting_map(
    child: &HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>,
    parent: &HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>,
) -> HashMap<DiagnosticCode, Sourced<DiagnosticSetting>> {
    let child_keys: HashSet<&DiagnosticCode> = child.keys().collect();
    let parent_keys: HashSet<&DiagnosticCode> = parent.keys().collect();
    let intersection = child_keys.intersection(&parent_keys).cloned().collect::<HashSet<_>>();
    let child_diff = child_keys.difference(&parent_keys).cloned().collect::<HashSet<_>>();
    let parent_diff = parent_keys.difference(&child_keys).cloned().collect::<HashSet<_>>();
    // For each key in the intersection, combine sources for repeated values
    // Otherwise take it from child
    // Then chain the set differences
    intersection.into_iter().map(|key| {
        let child_value = child.get(key).unwrap();
        let parent_value = parent.get(key).unwrap();
        if child_value.value == parent_value.value {
            (key.clone(), Sourced {
                value: child_value.value.clone(),
                sources: child_value.sources.clone().union(&parent_value.sources).cloned().collect(),
                info: String::new(),
            })
        } else {
            (key.clone(), child_value.clone())
        }
    }).chain(child_diff.into_iter().map(|key| {
        (key.clone(), child.get(key).unwrap().clone())
    })).chain(parent_diff.into_iter().map(|key| {
        (key.clone(), parent.get(key).unwrap().clone())
    })).collect::<HashMap<_, _>>()
}


fn apply_merge(child: &ConfigEntryRaw, parent: &ConfigEntryRaw) -> ConfigEntryRaw {
    let odoo_path = child.odoo_path.clone().or(parent.odoo_path.clone());
    let python_path = child.python_path.clone().or(parent.python_path.clone());
    // Simple combination of paths, sources will be merged after paths are processed
    let addons_paths = match child.addons_merge.clone().unwrap_or_default().value {
        MergeMethod::Merge => match (child.addons_paths.clone(), parent.addons_paths.clone()) {
            (Some(existing), Some(new)) => {
                Some(existing.into_iter().chain(new.into_iter()).collect())
            }
            (Some(paths), None) | (None, Some(paths)) => Some(paths),
            (None, None) => None,
        },
        MergeMethod::Override => child.addons_paths.clone(),
    };
    let additional_stubs = match child.additional_stubs_merge.clone().unwrap_or_default().value {
        MergeMethod::Merge => match (child.additional_stubs.clone(), parent.additional_stubs.clone()) {
            (Some(existing), Some(new)) => {
                Some(existing.into_iter().chain(new.into_iter()).collect())
            }
            (Some(paths), None) | (None, Some(paths)) => Some(paths),
            (None, None) => None,
        },
        MergeMethod::Override => child.additional_stubs.clone(),
    };
    let file_cache = child.file_cache.clone().or(parent.file_cache.clone());
    let diag_missing_imports = child.diag_missing_imports.clone().or(parent.diag_missing_imports.clone());
    let ac_filter_model_names = child.ac_filter_model_names.clone().or(parent.ac_filter_model_names.clone());
    let addons_merge = child.addons_merge.clone().or(parent.addons_merge.clone());
    let additional_stubs_merge = child.additional_stubs_merge.clone().or(parent.additional_stubs_merge.clone());
    let extends = child.extends.clone().or(parent.extends.clone());
    let auto_refresh_delay = child.auto_refresh_delay.clone().or(parent.auto_refresh_delay.clone());
    let version = child.version.clone().or(parent.version.clone());
    let base = child.base.clone().or(parent.base.clone());
    let diagnostic_settings = merge_sourced_diagnostic_setting_map(&child.diagnostic_settings, &parent.diagnostic_settings);
    let diagnostic_filters = child.diagnostic_filters.iter().chain(parent.diagnostic_filters.iter()).cloned().collect::<Vec<_>>();
    let stdlib = child.stdlib.clone().or(parent.stdlib.clone());
    let no_typeshed_stubs = child.no_typeshed_stubs.clone().or(parent.no_typeshed_stubs.clone());

    ConfigEntryRaw {
        name: child.name.clone(),
        odoo_path,
        python_path,
        addons_paths,
        additional_stubs,
        file_cache,
        diag_missing_imports,
        ac_filter_model_names,
        addons_merge,
        additional_stubs_merge,
        extends,
        auto_refresh_delay,
        version,
        base,
        diagnostic_settings,
        diagnostic_filters,
        stdlib,
        no_typeshed_stubs,
        ..Default::default()
    }
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
        let Some(entry) = config.get(key).cloned() else {
            continue
        };
        let Some(parent_entry) = entry.extends.as_ref().and_then(|key| config.get(key).cloned()) else {
            continue
        };
        config.insert(key.clone(), apply_merge(&entry, &parent_entry));
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
        let entry = match (child.get(&key), parent.get(&key)) {
            (Some(child), Some(parent)) => apply_merge(&child, &parent),
            (Some(entry), None) | (None, Some(entry))=> entry.clone(),
            (None, None) => continue, // unreachable
        };
        merged.insert(key, entry);
    }
    merged
}

fn load_config_from_file(path: String, ws_folders: &HashMap<String, String>,) -> Result<HashMap<String, ConfigEntryRaw>, String> {
    let path = PathBuf::from(path);
    if !path.exists() || !path.is_file() {
        return Err(S!(format!("Config file not found: {}", path.display())));
    }
    process_config(
        read_config_from_file(path)?,
        ws_folders,
        None,
    )
}

fn load_config_from_workspace(
    ws_folders: &HashMap<String, String>,
    workspace_name: &String,
    workspace_path: &String,
) -> Result<HashMap<String, ConfigEntryRaw>, String> {
    let mut current_dir = PathBuf::from(workspace_path);
    let mut visited_dirs = HashSet::new();
    let mut merged_config: HashMap<String, ConfigEntryRaw> = HashMap::new();
    merged_config.insert("default".to_string(), ConfigEntryRaw::new());

    loop {
        if !visited_dirs.insert(current_dir.clone()) {
            break;
        }

        let config_path = current_dir.join("odools.toml");
        if config_path.exists() && config_path.is_file() {
            let current_config = read_config_from_file(&config_path)?;
            merged_config = merge_configs(&merged_config, &current_config);
        }
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }
    let mut new_configs = vec![];
    for config in merged_config.values_mut() {
        // Make $base always an absolute path
        if let Some(base_var) = config.base.clone() {
            let base_path = PathBuf::from(base_var.value());

            // If $base ends with ${detectVersion}, match workspace path and set $version using path components
            if base_path.components().last().map(|c| c.as_os_str().to_string_lossy() == "${detectVersion}").unwrap_or(false)  {
                let Some(base_prefix_pb) = base_path.parent().map(PathBuf::from) else {
                    return Err(S!("$base must be a valid path with a parent directory"));
                };
                let abs_base = match base_prefix_pb.canonicalize() {
                    Ok(p) => p,
                    Err(e) => return Err(S!(format!("Failed to canonicalize base path: {} ({})", base_prefix_pb.display(), e))),
                };
                let base_prefix_pb = PathBuf::from(abs_base.sanitize());
                let ws_path_pb: PathBuf = PathBuf::from(workspace_path);
                let base_prefix_components: Vec<_> = base_prefix_pb.components().collect();
                let ws_path_components: Vec<_> = ws_path_pb.components().collect();
                if ws_path_components.len() > base_prefix_components.len()
                    && ws_path_components[..base_prefix_components.len()] == base_prefix_components[..] {
                    let version_component = ws_path_components[base_prefix_components.len()];
                    let version_str = version_component.as_os_str().to_string_lossy().to_string();
                    if !version_str.is_empty() {
                        config.version = Some(Sourced { value: version_str.clone(), ..Default::default() });
                        let mut resolved_base = base_prefix_pb.clone();
                        resolved_base.push(&version_str);
                        config.base.as_mut().map(|b| b.value = resolved_base.sanitize());
                    } else {
                        return Err(S!("Could not extract version from workspace path using $base"));
                    }
                } else {
                    return Err(S!("$base does not match the current workspace folder"));
                }
            }
        }
        // Also handle legacy $version with ${splitVersion}
        let Some(version_var) = config.version.clone() else {
            continue;
        };
        let version_path = PathBuf::from(version_var.value());
        if version_path.components().last().map(|c| c.as_os_str().to_string_lossy() == "${splitVersion}").unwrap_or(false) {
            config.abstract_ = true;
            let Some(parent_dir) = version_path.parent()  else {
                continue;
            };
            let Ok(parent_dir) = fill_or_canonicalize(
                &{Sourced { value: parent_dir.sanitize(), sources: version_var.sources.clone(), ..Default::default() }},
                ws_folders,
                Some(workspace_name),
                &|p| PathBuf::from(p).is_dir(),
                HashMap::new(),
            ) else {
                continue;
            };
            let parent_dir = PathBuf::from(parent_dir.value());
            for entry in fs::read_dir(parent_dir).into_iter().flatten().flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let dir_name = entry.file_name();
                let dir_name_str = dir_name.to_string_lossy();
                if VERSION_REGEX.is_match(&dir_name_str) {
                    let mut new_entry = config.clone();
                    new_entry.name = format!("{}-{}", config.name, dir_name_str);
                    new_entry.abstract_ = false;
                    new_entry.version = Some(Sourced { value: dir_name_str.to_string(), ..Default::default() });
                    new_entry.extends = Some(config.name.clone());
                    new_configs.push(new_entry);
                }
            }
        }
    }
    for new_entry in new_configs {
        merged_config.insert(new_entry.name.clone(), new_entry);
    }
    let merged_config = process_config(merged_config, ws_folders, Some(workspace_name))?;

    Ok(merged_config)
}

fn process_config(
    mut config_map: HashMap<String, ConfigEntryRaw>,
    ws_folders: &HashMap<String, String>,
    workspace_name: Option<&String>,
) -> Result<HashMap<String, ConfigEntryRaw>, String> {
    apply_extends(&mut config_map)?;
    // Process vars
    config_map.values_mut()
        .for_each(|entry| {
            // apply process_var to all vars
            if entry.abstract_ { return; }
            entry.version = entry.version.clone().map(|v| process_version(v, ws_folders, workspace_name));
        });
    // Process paths in the merged config
    config_map.values_mut()
        .for_each(|entry| {
            if entry.abstract_ { return; }
            process_paths(entry, ws_folders, workspace_name);
        });
    // Merge sourced paths
    config_map.values_mut()
        .for_each(|entry| {
            if entry.abstract_ { return; }
            entry.addons_paths = entry.addons_paths.clone().map(|paths| group_sourced_iters(paths).collect());
            entry.additional_stubs = entry.additional_stubs.clone().map(|stubs| group_sourced_iters(stubs).collect());
        });

    Ok(config_map)
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
            merged_entry.extends = match (merged_entry.extends.clone(), raw_entry.extends) {
                (Some(existing), Some(new)) if existing != new => {
                    return Err(S!(format!("Conflict in 'extends' for profile '{}': '{}' vs '{}'", key, existing, new)));
                }
                (existing, new) => new.or(existing),
            };
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
            merged_entry.addons_paths = match (merged_entry.addons_paths.clone(), raw_entry.addons_paths.clone()) {
                (Some(existing), Some(new)) => {
                    Some(merge_sourced_iters(existing, new).collect())
                }
                (Some(paths), None) | (None, Some(paths)) => Some(paths),
                (None, None) => None,
            };
            merged_entry.additional_stubs = match (merged_entry.additional_stubs.clone(), raw_entry.additional_stubs.clone()) {
                (Some(existing), Some(new)) => {
                    Some(merge_sourced_iters(existing, new).collect())
                }
                (Some(paths), None) | (None, Some(paths)) => Some(paths),
                (None, None) => None,
            };
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
            merged_entry.auto_refresh_delay = merge_sourced_options(
                merged_entry.auto_refresh_delay.clone(),
                raw_entry.auto_refresh_delay.clone(),
                key.clone(),
                "auto_refresh_delay".to_string(),
            )?;
            merged_entry.diagnostic_settings = merge_sourced_diagnostic_setting_map(
                &merged_entry.diagnostic_settings,
                &raw_entry.diagnostic_settings,
            );
            merged_entry.abstract_ = merged_entry.abstract_ || raw_entry.abstract_;
            merged_entry.diagnostic_filters.extend(raw_entry.diagnostic_filters.iter().cloned());
            merged_entry.stdlib =  merge_sourced_options(
                merged_entry.stdlib.clone(),
                raw_entry.stdlib.clone(),
                key.clone(),
                "stdlib".to_string(),
            )?;
            merged_entry.no_typeshed_stubs =  merge_sourced_options(
                merged_entry.no_typeshed_stubs.clone(),
                raw_entry.no_typeshed_stubs.clone(),
                key.clone(),
                "no_typeshed_stubs".to_string(),
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
                    entry.odoo_path = Some(Sourced { value: path.clone(), sources: HashSet::from([S!(format!("$workspaceFolder:{name}"))]), ..Default::default()});
                }
            }
        }
    }

    let config_file = ConfigFile { config: merged_raw_config.values().cloned().collect::<Vec<_>>()};

    // Convert the merged ConfigEntryRaw structure into ConfigEntry
    let mut final_config: ConfigNew = HashMap::new();
    for (key, raw_entry) in merged_raw_config {
        final_config.insert(
            key.clone(),
            ConfigEntry {
                name: key.clone(),
                odoo_path: raw_entry.odoo_path.map(|op| op.value),
                addons_paths: raw_entry.addons_paths.into_iter().flatten().map(|op| op.value).collect(),
                python_path: raw_entry.python_path.map(|op| op.value).unwrap_or(S!(get_python_command().unwrap_or_default())),
                additional_stubs: raw_entry.additional_stubs.into_iter().flatten().map(|op| op.value).collect(),
                file_cache: raw_entry.file_cache.map(|op| op.value).unwrap_or(true),
                diag_missing_imports: raw_entry.diag_missing_imports.map(|op| op.value).unwrap_or_default(),
                ac_filter_model_names: raw_entry.ac_filter_model_names.map(|op| op.value).unwrap_or(true),
                auto_refresh_delay: clamp_auto_refresh_delay(raw_entry.auto_refresh_delay.map(|op| op.value).unwrap_or(1000)),
                abstract_: raw_entry.abstract_,
                diagnostic_settings: raw_entry.diagnostic_settings.into_iter()
                    .map(|(k, v)| (k, v.value))
                    .collect(),
                diagnostic_filters: raw_entry.diagnostic_filters.into_iter().map(|f| f.value).collect(),
                stdlib: raw_entry.stdlib.into_iter().map(|f| f.value).collect(),
                no_typeshed_stubs: raw_entry.no_typeshed_stubs.map(|f| f.value).unwrap_or_default(),
                ..Default::default()
            },
        );
    }

    Ok((final_config, config_file))
}

pub fn get_configuration(ws_folders: &HashMap<String, String>, cli_config_file: &Option<String>)  -> Result<(ConfigNew, ConfigFile), String> {
    let mut ws_confs: Vec<HashMap<String, ConfigEntryRaw>> = Vec::new();

    if let Some(path) = cli_config_file {
        let config_from_file = load_config_from_file(path.clone(), ws_folders)?;
        ws_confs.push(config_from_file);
    }

    let ws_confs_result: Result<Vec<_>, _> = ws_folders
        .iter()
        .map(|ws_f| load_config_from_workspace(ws_folders, ws_f.0, ws_f.1))
        .collect();

    ws_confs.extend(ws_confs_result?);

    merge_all_workspaces(ws_confs, ws_folders)
}

/// Check if the old and new configuration entries are different enough to require a restart.
/// Only changes in the odoo_path, addons_paths, python_path, and additional_stubs are considered significant.
pub fn needs_restart(old: &ConfigEntry, new: &ConfigEntry) -> bool {
    old.odoo_path != new.odoo_path ||
    old.addons_paths != new.addons_paths ||
    old.python_path != new.python_path ||
    old.additional_stubs != new.additional_stubs ||
    old.stdlib != new.stdlib ||
    old.no_typeshed_stubs != new.no_typeshed_stubs

}

fn clamp_auto_refresh_delay(val: u64) -> u64 {
    if val < 1000 {
        1000
    } else if val > 15000 {
        15000
    } else {
        val
    }
}
