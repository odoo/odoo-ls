//! `ConfigEntry` — the runtime configuration: a resolved `Profile` (the
//! `ConfigKey → ConfigValue` map) plus typed accessors.
//!
//! Two accessor layers:
//!  - **Generic** (`as_bool`/`as_u64`/`as_str`/`opt_str`/`str_set`): the
//!    canonical API. A new field needs no code here — just read it with its key.
//!  - **Named** (`file_cache()`, `odoo_path()`, …): thin shims over the generic
//!    layer for the existing fields, so call sites read ergonomically. New
//!    fields do not require a named accessor.

use std::str::FromStr;

use crate::core::config::ConfigKey;
use crate::core::diagnostics::{DiagnosticCode, DiagnosticSetting};
use crate::utils::{HashMap, HashSet, default_python_command};

use super::spec::ConfigFieldSpecKind;
use super::spec::registry;
use super::value::{ConfigValue, DiagMissingImportsMode, DiagnosticFilter, Profile, Sourced};

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub name: String,
    abstract_: bool,
    values: HashMap<ConfigKey, ConfigValue>,
}

impl Default for ConfigEntry {
    fn default() -> Self {
        let default_values = {
            registry()
                .keys()
                .filter_map(|&config_key| {
                    let default = config_key.default_value()?;
                    Some((config_key, default))
                })
                .collect()
        };
        Self {
            name: super::value::DEFAULT_PROFILE_NAME.to_string(),
            abstract_: false,
            values: default_values,
        }
    }
}

impl ConfigEntry {
    pub(super) fn from_profile(profile: Profile) -> Self {
        ConfigEntry {
            name: profile.name,
            abstract_: profile.abstract_,
            values: profile.values,
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_abstract(&self) -> bool {
        self.abstract_
    }

    // --- Generic getters (canonical API) ---

    pub fn as_bool(&self, key: ConfigKey) -> bool {
        debug_assert!(
            matches!(key.kind(), super::spec::ConfigFieldSpecKind::Bool),
            "as_bool on non-bool key {key:?}"
        );
        self.values
            .get(&key)
            .and_then(ConfigValue::as_bool)
            .or_else(|| key.default_value().and_then(|v| v.as_bool()))
            .unwrap_or(false)
    }
    pub fn as_u64(&self, key: ConfigKey) -> u64 {
        debug_assert!(matches!(key.kind(), super::spec::ConfigFieldSpecKind::U64));
        self.values
            .get(&key)
            .and_then(ConfigValue::as_u64)
            .or_else(|| key.default_value().and_then(|v| v.as_u64()))
            .unwrap_or(0)
    }
    /// Also serves enum-valued keys stored as `Str` (e.g. `diag_missing_imports`),
    /// so it asserts no kind. Absent → `""`; use `opt_str` for a different default.
    pub fn as_str(&self, key: ConfigKey) -> &str {
        self.values
            .get(&key)
            .and_then(ConfigValue::as_str)
            .unwrap_or("")
    }
    pub fn opt_str(&self, key: ConfigKey) -> Option<&str> {
        self.values.get(&key).and_then(ConfigValue::as_str)
    }
    pub fn str_set(&self, key: ConfigKey) -> HashSet<String> {
        self.values
            .get(&key)
            .and_then(ConfigValue::as_list)
            .map(|l| l.iter().map(|s| s.value().clone()).collect())
            .unwrap_or_default()
    }

    // --- Mutators (used by CLI arg reconciliation) ---

    pub fn set_str(&mut self, key: ConfigKey, value: String) {
        debug_assert!(
            matches!(
                key.kind(),
                ConfigFieldSpecKind::Str
                    | ConfigFieldSpecKind::DiagImportsMode
                    | ConfigFieldSpecKind::MergeMethod
            ),
            "set_str on non-string key {key:?}"
        );
        self.values.insert(key, ConfigValue::str(value));
    }
    pub fn set_bool(&mut self, key: ConfigKey, value: bool) {
        debug_assert!(
            matches!(key.kind(), ConfigFieldSpecKind::Bool),
            "set_bool on non-bool key {key:?}"
        );
        self.values.insert(key, ConfigValue::bool(value));
    }
    pub fn set_u64(&mut self, key: ConfigKey, value: u64) {
        debug_assert!(
            matches!(key.kind(), ConfigFieldSpecKind::U64),
            "set_u64 on non-u64 key {key:?}"
        );
        self.values.insert(key, ConfigValue::u64(value));
    }
    pub fn set_string_list(&mut self, key: ConfigKey, items: impl IntoIterator<Item = String>) {
        debug_assert!(
            matches!(key.kind(), ConfigFieldSpecKind::StrList),
            "set_string_list on non-list key {key:?}"
        );
        let list = items
            .into_iter()
            .map(|value| Sourced::new(value, "$override"))
            .collect();
        self.values.insert(key, ConfigValue::List(list));
    }
    pub fn extend_string_list(&mut self, key: ConfigKey, items: impl IntoIterator<Item = String>) {
        debug_assert!(
            matches!(key.kind(), ConfigFieldSpecKind::StrList),
            "extend_string_list on non-list key {key:?}"
        );
        let mut current: Vec<_> = self
            .values
            .get(&key)
            .and_then(ConfigValue::as_list)
            .cloned()
            .unwrap_or_default();
        for item in items {
            current.push(Sourced::new(item, "$cli"));
        }
        self.values.insert(key, ConfigValue::List(current));
    }

    // --- Named accessors for the existing fields (shims) ---

    pub fn file_cache(&self) -> bool {
        self.as_bool(ConfigKey::FileCache)
    }
    pub fn ac_filter_model_names(&self) -> bool {
        self.as_bool(ConfigKey::AcFilterModelNames)
    }
    pub fn no_typeshed_stubs(&self) -> bool {
        self.as_bool(ConfigKey::NoTypeshedStubs)
    }
    /// Auto-refresh delay, clamped to `[1000, 15000]` ms — the authoritative bound
    /// for this key (the generic `as_u64` returns the raw, unclamped value).
    pub fn auto_refresh_delay(&self) -> u64 {
        self.as_u64(ConfigKey::AutoRefreshDelay).clamp(1000, 15000)
    }
    pub fn diag_missing_imports(&self) -> DiagMissingImportsMode {
        self.opt_str(ConfigKey::DiagMissingImports)
            .and_then(|s| DiagMissingImportsMode::from_str(s).ok())
            .unwrap_or_default()
    }
    pub fn python_path(&self) -> String {
        self.opt_str(ConfigKey::PythonPath)
            .map(str::to_string)
            .unwrap_or_else(default_python_command)
    }
    pub fn stdlib(&self) -> String {
        self.opt_str(ConfigKey::Stdlib).unwrap_or("").to_string()
    }
    pub fn odoo_path(&self) -> Option<String> {
        self.opt_str(ConfigKey::OdooPath).map(str::to_string)
    }
    pub fn addons_paths(&self) -> HashSet<String> {
        self.str_set(ConfigKey::AddonsPaths)
    }
    pub fn additional_stubs(&self) -> HashSet<String> {
        self.str_set(ConfigKey::AdditionalStubs)
    }
    pub fn additional_languages(&self) -> HashSet<String> {
        self.str_set(ConfigKey::AdditionalLanguages)
    }
    pub fn diagnostic_settings(&self) -> HashMap<DiagnosticCode, DiagnosticSetting> {
        self.values
            .get(&ConfigKey::DiagnosticSettings)
            .and_then(ConfigValue::as_diag_settings)
            .map(|m| m.iter().map(|(k, v)| (*k, *v.value())).collect())
            .unwrap_or_default()
    }
    pub fn diagnostic_filters(&self) -> Vec<DiagnosticFilter> {
        self.values
            .get(&ConfigKey::DiagnosticFilters)
            .and_then(ConfigValue::as_diag_filters)
            .map(|v| v.iter().map(|f| f.value().clone()).collect())
            .unwrap_or_default()
    }
}

/// Whether the change between two resolved configs requires a server restart:
/// any `restart`-flagged field whose value differs.
pub fn needs_restart(old: &ConfigEntry, new: &ConfigEntry) -> bool {
    ConfigKey::all()
        .iter()
        .filter(|k| registry().get(k).map(|s| s.restart).unwrap_or(false))
        .any(|&k| !value_eq(old.values.get(&k), new.values.get(&k)))
}

/// Compare two optional config values. Sufficient for the
/// restart-flagged keys (scalars and string lists).
fn value_eq(a: Option<&ConfigValue>, b: Option<&ConfigValue>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(ConfigValue::Scalar(x)), Some(ConfigValue::Scalar(y))) => x.value() == y.value(),
        (Some(ConfigValue::List(x)), Some(ConfigValue::List(y))) => {
            let xs: HashSet<&String> = x.iter().map(|s| s.value()).collect();
            let ys: HashSet<&String> = y.iter().map(|s| s.value()).collect();
            xs == ys
        }
        // An unset list and an explicitly-empty list are equivalent: a list field
        // toggling between the two must not spuriously flag a restart.
        (None, Some(ConfigValue::List(l))) | (Some(ConfigValue::List(l)), None) => l.is_empty(),
        _ => false,
    }
}
