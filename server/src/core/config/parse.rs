//! Spec-driven TOML parsing: read an `odools.toml` into `Profile`s using the
//! field registry. Parsing is driven by `specs()` — it dispatches on each key's
//! `ConfigFieldSpecKind`, so it grows by kind, never per-field. Unknown keys are rejected.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use tracing::warn;

use crate::core::config::ConfigKey;
use crate::core::diagnostics::{DiagnosticCode, DiagnosticSetting};
use crate::utils::{HashMap, HashSet, PathSanitizer};

use super::spec::ConfigFieldSpecKind;
use super::value::DEFAULT_PROFILE_NAME;
use super::value::{
    ConfigValue, DiagMissingImportsMode, DiagnosticFilter, MergeMethod, Profile, Scalar, Sourced,
};

/// Parse one config file into its profiles, tagging every value with `path`.
pub(super) fn parse_file(path: &Path) -> Result<Vec<Profile>, String> {
    let contents = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.sanitize()))?;
    let source = path.sanitize();
    let root: toml::Value =
        toml::from_str(&contents).map_err(|e| format!("{source}: {e}"))?;

    let mut profiles = Vec::new();
    let entries = match root.get("config") {
        // No `config` array at all: an empty file is valid (no profiles).
        None => return Ok(profiles),
        // A `config` key of the wrong shape is a mistake, not "no config".
        Some(value) => value.as_array().ok_or_else(|| {
            format!("{source}: 'config' must be an array of tables ([[config]])")
        })?,
    };
    for entry in entries {
        profiles.push(parse_entry(entry, &source)?);
    }
    Ok(profiles)
}

fn parse_entry(entry: &toml::Value, source: &str) -> Result<Profile, String> {
    let table = entry
        .as_table()
        .ok_or_else(|| format!("{source}: a [[config]] entry must be a table"))?;

    let name = match table.get("name") {
        Some(v) => as_str_field(v, "name")
            .map_err(|e| format!("{source}: {e}"))?
            .to_string(),
        None => DEFAULT_PROFILE_NAME.to_string(),
    };
    let mut profile = Profile::new(name);
    profile.extends = match table.get("extends") {
        Some(v) => Some(
            as_str_field(v, "extends")
                .map_err(|e| format!("{source}: profile '{}': {e}", profile.name))?
                .to_string(),
        ),
        None => None,
    };

    for (raw_key, value) in table {
        if raw_key == "name" || raw_key == "extends" {
            continue;
        }
        let Some(key) = ConfigKey::from_name(raw_key) else {
            // An unknown key is ignored rather than fatal: a typo, a forward- or
            // backward-compatibility key, or a panel-only field (e.g. `abstract`)
            // must not discard the user's entire configuration. It is still
            // reported (not just logged) so the user can spot the typo.
            let msg = format!("unknown config key '{raw_key}' in {source} (ignored)");
            warn!("{msg}");
            profile.warnings.push(msg);
            continue;
        };
        let parsed = parse_value(key, value, source)
            .map_err(|e| format!("{source}: profile '{}': {e}", profile.name))?;
        if let Some(parsed) = parsed {
            profile.values.insert(key, parsed);
        }
    }
    Ok(profile)
}

fn scalar(value: Scalar, source: &str) -> ConfigValue {
    ConfigValue::scalar(value, HashSet::from_iter([source.to_string()]))
}

/// Read a TOML value as a string, or report a typed error naming the field.
fn as_str_field<'a>(value: &'a toml::Value, name: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("'{name}' must be a string"))
}

/// Parse a single value according to its key's `ConfigFieldSpecKind`.
fn parse_value(
    key: ConfigKey,
    value: &toml::Value,
    source: &str,
) -> Result<Option<ConfigValue>, String> {
    let name = key.as_str();
    match key.kind() {
        ConfigFieldSpecKind::Bool => {
            let b = value
                .as_bool()
                .ok_or_else(|| format!("'{name}' must be a boolean"))?;
            Ok(Some(scalar(Scalar::Bool(b), source)))
        }
        ConfigFieldSpecKind::U64 => {
            let n = value
                .as_integer()
                .ok_or_else(|| format!("'{name}' must be an integer"))?;
            let n: u64 = n
                .try_into()
                .map_err(|_| format!("'{name}' must be a non-negative integer"))?;
            Ok(Some(scalar(Scalar::U64(n), source)))
        }
        ConfigFieldSpecKind::Str => {
            let s = as_str_field(value, name)?;
            Ok(Some(scalar(Scalar::Str(s.to_string()), source)))
        }
        ConfigFieldSpecKind::DiagImportsMode => {
            let s = as_str_field(value, name)?;
            // Validate the enum value.
            DiagMissingImportsMode::from_str(s)
                .map_err(|_| format!("invalid value '{s}' for '{name}'"))?;
            Ok(Some(scalar(Scalar::Str(s.to_string()), source)))
        }
        ConfigFieldSpecKind::MergeMethod => {
            let s = as_str_field(value, name)?;
            // Validate the enum value (the valid set lives on `MergeMethod`).
            MergeMethod::from_str(s).map_err(|_| format!("invalid value '{s}' for '{name}'"))?;
            Ok(Some(scalar(Scalar::Str(s.to_string()), source)))
        }
        ConfigFieldSpecKind::StrList => {
            let arr = value
                .as_array()
                .ok_or_else(|| format!("'{name}' must be an array"))?;
            let mut items = Vec::with_capacity(arr.len());
            for el in arr {
                let s = el
                    .as_str()
                    .ok_or_else(|| format!("'{name}' entries must be strings"))?;
                items.push(Sourced::new(s.to_string(), source));
            }
            Ok(Some(ConfigValue::List(items)))
        }
        ConfigFieldSpecKind::DiagSettings => {
            let map: HashMap<DiagnosticCode, DiagnosticSetting> = value
                .clone()
                .try_into()
                .map_err(|e: toml::de::Error| e.to_string())?;
            let sourced = map
                .into_iter()
                .map(|(k, v)| (k, Sourced::new(v, source)))
                .collect();
            Ok(Some(ConfigValue::DiagSettings(sourced)))
        }
        ConfigFieldSpecKind::DiagFilters => {
            let filters: Vec<DiagnosticFilter> = value
                .clone()
                .try_into()
                .map_err(|e: toml::de::Error| e.to_string())?;
            let sourced = filters
                .into_iter()
                .map(|f| Sourced::new(f, source))
                .collect();
            Ok(Some(ConfigValue::DiagFilters(sourced)))
        }
    }
}
