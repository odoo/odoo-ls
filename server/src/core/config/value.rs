//! The config **data model**: the runtime value types and the per-profile state
//! the pipeline operates on.
//!
//! Layered below `spec.rs` and `build.rs` (it depends on neither): public value
//! enums, the `Sourced<T>` provenance wrapper and its merge helpers, the
//! internal `Scalar`/`ConfigValue` representation, and the `Profile` working
//! structure. The field-declaration machinery (`FieldSpec`, the registry) lives
//! in `spec.rs`.
use std::hash::Hash;
use std::path::Path;

use glob::Pattern;
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};

use crate::S;
use crate::core::config::ConfigKey;
use crate::core::diagnostics::{DiagnosticCode, DiagnosticSetting};
use crate::utils::{HashMap, HashSet, PathSanitizer};

/// Default profile name when a `[[config]]` entry omits `name`.
pub const DEFAULT_PROFILE_NAME: &str = "default";

/// Provenance tag for compiler-supplied defaults (`FieldSpec::default` and the
/// `Default` impls). Centralized so the `$default` literal lives in one place.
pub(crate) const DEFAULT_SOURCE: &str = "$default";

/// The single-element source set used for compiler-supplied defaults.
pub(crate) fn default_sources() -> HashSet<String> {
    HashSet::from_iter([S!(DEFAULT_SOURCE)])
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiagMissingImportsMode {
    None,
    OnlyOdoo,
    #[default]
    All,
}

impl std::str::FromStr for DiagMissingImportsMode {
    type Err = ();

    fn from_str(input: &str) -> Result<DiagMissingImportsMode, Self::Err> {
        match input {
            "none" => Ok(DiagMissingImportsMode::None),
            "only_odoo" => Ok(DiagMissingImportsMode::OnlyOdoo),
            "all" => Ok(DiagMissingImportsMode::All),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticFilterPathType {
    #[default]
    In,
    NotIn,
}

/// How a controlled list field combines with its parent (see
/// `MergeRule::ListControlledBy`). Stored as its serde name in a `Scalar::Str`
/// and parsed on demand, so the valid set lives only here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeMethod {
    #[default]
    Merge,
    Override,
}

impl MergeMethod {
    /// The accepted serialized values (used for parsing and JSON-schema emission).
    pub(crate) const VALUES: [&'static str; 2] = ["merge", "override"];
}

impl std::str::FromStr for MergeMethod {
    type Err = ();

    fn from_str(input: &str) -> Result<MergeMethod, Self::Err> {
        match input {
            "merge" => Ok(MergeMethod::Merge),
            "override" => Ok(MergeMethod::Override),
            _ => Err(()),
        }
    }
}

// ---------------------------------------------------------------------------
// DiagnosticFilter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DiagnosticFilter {
    pub paths: Vec<Pattern>,
    pub codes: Vec<Regex>,
    pub types: Vec<DiagnosticSetting>,
    pub path_type: DiagnosticFilterPathType,
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
            let sanitized_path = Path::new(path).sanitize_cow();
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
                return Err(serde::de::Error::custom(
                    "DiagnosticFilter.types cannot contain 'Disabled'",
                ));
            }
        }
        Ok(DiagnosticFilter {
            paths,
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
        let mut s = serializer.serialize_struct("DiagnosticFilter", 4)?;
        let paths: Vec<String> = self.paths.iter().map(|p| p.as_str().to_string()).collect();
        s.serialize_field("paths", &paths)?;
        let codes: Vec<String> = self.codes.iter().map(|r| r.as_str().to_string()).collect();
        s.serialize_field("codes", &codes)?;
        s.serialize_field("types", &self.types)?;
        s.serialize_field("path_type", &self.path_type)?;
        s.end()
    }
}

// ---------------------------------------------------------------------------
// Sourced<T> — wraps a value with provenance tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Sourced<T> {
    pub(crate) value: T,
    pub(crate) sources: HashSet<String>,
    pub(crate) info: String,
}

impl<T> Sourced<T> {
    /// A value with an explicit (already-resolved) set of sources and no info.
    /// The canonical constructor — `new`/`defaulted` and every other site build
    /// `Sourced` through this rather than the struct literal.
    pub(crate) fn with_sources(value: T, sources: HashSet<String>) -> Self {
        Sourced {
            value,
            sources,
            info: String::new(),
        }
    }
    /// A value tagged with a single source (and no info).
    pub(crate) fn new(value: T, source: impl Into<String>) -> Self {
        Sourced::with_sources(value, HashSet::from_iter([source.into()]))
    }
    /// A value tagged as a compiler-supplied default (`$default` provenance).
    pub(crate) fn defaulted(value: T) -> Self {
        Sourced::with_sources(value, default_sources())
    }
    /// Attach an informational note (surfaced in the config panel, e.g. to
    /// explain why a value was rejected).
    pub(crate) fn with_info(mut self, info: String) -> Self {
        self.info = info;
        self
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn sources(&self) -> &HashSet<String> {
        &self.sources
    }
}

impl<T: Default> Default for Sourced<T> {
    fn default() -> Self {
        Sourced::defaulted(T::default())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Sourced<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = T::deserialize(deserializer)?;
        // Sources are attached later by the parser; deserialize with none.
        Ok(Sourced::with_sources(value, HashSet::default()))
    }
}

// ---------------------------------------------------------------------------
// Sourced merge helpers
// ---------------------------------------------------------------------------

/// Groups `Sourced<T>` items by their value, merging their sources. The output
/// is sorted by value so the result is deterministic run-to-run (the grouping is
/// backed by a `HashMap`, which is not) — the config panel and any snapshot test
/// comparing rendered list output rely on a stable order.
pub(crate) fn group_sourced_iters<T, I>(iter: I) -> impl Iterator<Item = Sourced<T>>
where
    T: Clone + Eq + Hash + Ord,
    I: IntoIterator<Item = Sourced<T>>,
{
    iter.into_iter()
        .into_group_map_by(|s| s.value.clone())
        .into_iter()
        .map(|(value, group)| {
            let sources = group.into_iter().flat_map(|s| s.sources).collect();
            Sourced::with_sources(value, sources)
        })
        .sorted_by(|a, b| a.value.cmp(&b.value))
}

// ---------------------------------------------------------------------------
// Diagnostic settings merge
// ---------------------------------------------------------------------------

/// Merge two `diagnostic_settings` maps. A child setting overrides the parent's,
/// except when both name the same value — then it is kept and their sources are
/// unioned.
pub(super) fn merge_sourced_diagnostic_setting_map(
    child: &HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>,
    parent: &HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>,
) -> HashMap<DiagnosticCode, Sourced<DiagnosticSetting>> {
    let mut merged = parent.clone();
    for (&code, child_setting) in child {
        match merged.get_mut(&code) {
            Some(parent_setting) if parent_setting.value == child_setting.value => {
                parent_setting
                    .sources
                    .extend(child_setting.sources.iter().cloned());
            }
            _ => {
                merged.insert(code, child_setting.clone());
            }
        }
    }
    merged
}

/// Whether a source string is a real filesystem path (an absolute Unix path or a
/// Windows `C:`-style drive path) rather than a `$`-prefixed provenance token.
pub(super) fn is_fs_path(s: &str) -> bool {
    s.starts_with('/') || s.as_bytes().get(1) == Some(&b':')
}

/// The directory of the config file a value came from (for relative resolution).
/// Shared by the path-canonicalization and version stage hooks.
///
/// A value can carry several sources (a filesystem path plus `$`-prefixed tokens
/// like `$default` or `$workspaceFolder:…`). Relative resolution only makes sense
/// against a real config-file path, so prefer those; `$`-tokens sort before `/`
/// and would otherwise win a naive `min()`.
pub(super) fn config_dir(sources: &HashSet<String>) -> &Path {
    sources
        .iter()
        .filter(|s| is_fs_path(s))
        .min()
        .or_else(|| sources.iter().min())
        .map(Path::new)
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."))
}

// ---------------------------------------------------------------------------
// Configuration Value model
// ---------------------------------------------------------------------------

/// A resolved scalar. Enum-valued settings (e.g. `diag_missing_imports`,
/// `addons_merge`) are stored as `Str` (their serde name) and parsed on demand.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Scalar {
    Bool(bool),
    U64(u64),
    Str(String),
}

impl std::fmt::Display for Scalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scalar::Bool(b) => write!(f, "{b}"),
            Scalar::U64(n) => write!(f, "{n}"),
            Scalar::Str(s) => write!(f, "{s}"),
        }
    }
}

/// A config value with provenance: scalars carry one `Sourced`; lists carry
/// per-element sources.
#[derive(Debug, Clone)]
pub(super) enum ConfigValue {
    Scalar(Sourced<Scalar>),
    List(Vec<Sourced<String>>),
    // ---Custom Behavior Types---
    DiagSettings(HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>),
    DiagFilters(Vec<Sourced<DiagnosticFilter>>),
}

impl ConfigValue {
    fn as_scalar(&self) -> Option<&Sourced<Scalar>> {
        match self {
            ConfigValue::Scalar(s) => Some(s),
            _ => None,
        }
    }
    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self.as_scalar()?.value() {
            Scalar::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub(crate) fn as_u64(&self) -> Option<u64> {
        match self.as_scalar()?.value() {
            Scalar::U64(n) => Some(*n),
            _ => None,
        }
    }
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self.as_scalar()?.value() {
            Scalar::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub(crate) fn as_list(&self) -> Option<&Vec<Sourced<String>>> {
        match self {
            ConfigValue::List(l) => Some(l),
            _ => None,
        }
    }
    pub(crate) fn as_diag_settings(
        &self,
    ) -> Option<&HashMap<DiagnosticCode, Sourced<DiagnosticSetting>>> {
        match self {
            ConfigValue::DiagSettings(m) => Some(m),
            _ => None,
        }
    }
    pub(crate) fn as_diag_filters(&self) -> Option<&Vec<Sourced<DiagnosticFilter>>> {
        match self {
            ConfigValue::DiagFilters(v) => Some(v),
            _ => None,
        }
    }

    /// A scalar value with an explicit set of sources.
    pub(crate) fn scalar(value: Scalar, sources: HashSet<String>) -> ConfigValue {
        ConfigValue::Scalar(Sourced::with_sources(value, sources))
    }
    pub(crate) fn bool(b: bool) -> ConfigValue {
        ConfigValue::Scalar(Sourced::defaulted(Scalar::Bool(b)))
    }
    pub(crate) fn u64(n: u64) -> ConfigValue {
        ConfigValue::Scalar(Sourced::defaulted(Scalar::U64(n)))
    }
    pub(crate) fn str(s: impl Into<String>) -> ConfigValue {
        ConfigValue::Scalar(Sourced::defaulted(Scalar::Str(s.into())))
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

/// The resolved values of one profile, keyed by config key.
pub(crate) type ValuesMap = HashMap<ConfigKey, ConfigValue>;

/// A single `[[config]]` profile being built. `name`/`extends`/`abstract_` are
/// structural metadata; everything else lives in `values`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Profile {
    pub name: String,
    pub extends: Option<String>,
    pub abstract_: bool,
    pub values: ValuesMap,
    // Retained to be shown in the config panel, but not used by the runtime
    pub rejected: HashMap<ConfigKey, Vec<Sourced<String>>>,
}

impl Profile {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Profile {
            name: name.into(),
            ..Default::default()
        }
    }
    pub(crate) fn get(&self, key: ConfigKey) -> Option<&ConfigValue> {
        self.values.get(&key)
    }

    /// Get scalar string (`Str`-typed) value  by key with its provenance, if present.
    pub(crate) fn scalar_str(&self, key: ConfigKey) -> Option<Sourced<String>> {
        match self.get(key)? {
            ConfigValue::Scalar(s) => match s.value() {
                Scalar::Str(v) => Some(Sourced::with_sources(v.clone(), s.sources().clone())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Set a `Str`-typed scalar field with explicit provenance.
    pub(crate) fn set_scalar_str(
        &mut self,
        key: ConfigKey,
        value: String,
        sources: HashSet<String>,
    ) {
        self.values
            .insert(key, ConfigValue::scalar(Scalar::Str(value), sources));
    }

    /// Record a user-provided value that failed resolution
    pub(crate) fn add_rejected(
        &mut self,
        key: ConfigKey,
        value: String,
        sources: HashSet<String>,
        reason: String,
    ) {
        self.rejected
            .entry(key)
            .or_default()
            .push(Sourced::with_sources(value, sources).with_info(reason));
    }
}

/// All profiles, keyed by profile name.
pub(crate) type ProfileSet = HashMap<String, Profile>;

/// Context threaded through the pipeline (workspace folders being resolved).
pub(crate) struct PipelineCtx<'a> {
    pub unique_ws_folders: &'a HashMap<String, String>,
    pub current_ws: Option<&'a (String, String)>,
}
