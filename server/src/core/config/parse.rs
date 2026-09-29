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

    // A non-string `name` is used as its raw text, with a warning, rather than
    // failing the config or being merged into "default".
    let (name, name_warning) = match table.get("name") {
        Some(v) => match v.as_str() {
            Some(s) => (s.to_string(), None),
            None => {
                let raw = raw_text(v);
                let msg = format!("'name' must be a string in {source}, using '{raw}'");
                (raw, Some(msg))
            }
        },
        None => (DEFAULT_PROFILE_NAME.to_string(), None),
    };
    let mut profile = Profile::new(name);
    profile.warnings.extend(name_warning);
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
        let mut rejected_entries = Vec::new();
        match parse_value(key, value, source, &mut rejected_entries) {
            Ok(Some(parsed)) => {
                profile.values.insert(key, parsed);
            }
            Ok(None) => {}
            Err(e) => rejected_entries.push((raw_text(value), e)),
        }
        for (raw, reason) in rejected_entries {
            // toml errors end with a newline.
            let reason = reason.trim_end().to_string();
            profile.add_rejected(key, raw, HashSet::from_iter([source.to_string()]), reason);
        }
    }
    Ok(profile)
}

/// A TOML value as shown in a rejection: strings unquoted, others as TOML.
fn raw_text(value: &toml::Value) -> String {
    value.as_str().map_or_else(|| value.to_string(), str::to_string)
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
/// Invalid list entries are pushed to `rejected` as `(raw, reason)` and skipped;
/// `Err` rejects the whole value.
fn parse_value(
    key: ConfigKey,
    value: &toml::Value,
    source: &str,
    rejected: &mut Vec<(String, String)>,
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
                match el.as_str() {
                    Some(s) => items.push(Sourced::new(s.to_string(), source)),
                    None => rejected.push((raw_text(el), format!("'{name}' entries must be strings"))),
                }
            }
            Ok(Some(ConfigValue::List(items)))
        }
        ConfigFieldSpecKind::DiagSettings => {
            let table = value
                .as_table()
                .ok_or_else(|| format!("'{name}' must be a table"))?;
            let mut settings = HashMap::default();
            for (code, setting) in table {
                let parsed = DiagnosticCode::from_str(code)
                    .map_err(|_| format!("unknown diagnostic code '{code}'"))
                    .and_then(|c| {
                        let s: DiagnosticSetting = setting
                            .clone()
                            .try_into()
                            .map_err(|e: toml::de::Error| e.to_string())?;
                        Ok((c, s))
                    });
                match parsed {
                    Ok((c, s)) => {
                        settings.insert(c, Sourced::new(s, source));
                    }
                    Err(e) => rejected.push((format!("{code} = {}", raw_text(setting)), e)),
                }
            }
            Ok(Some(ConfigValue::DiagSettings(settings)))
        }
        ConfigFieldSpecKind::DiagFilters => {
            let arr = value
                .as_array()
                .ok_or_else(|| format!("'{name}' must be an array"))?;
            let mut filters = Vec::with_capacity(arr.len());
            for el in arr {
                match el.clone().try_into::<DiagnosticFilter>() {
                    Ok(f) => filters.push(Sourced::new(f, source)),
                    Err(e) => rejected.push((raw_text(el), e.to_string())),
                }
            }
            Ok(Some(ConfigValue::DiagFilters(filters)))
        }
    }
}
