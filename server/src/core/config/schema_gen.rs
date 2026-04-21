//! Spec-driven JSON schema for `odools.toml`, generated from `specs()`.
//! Describes the TOML file format independently of internal storage, so it stays
//! correct with no per-field edit. Consumed by the `print_config_schema` binary.

use serde_json::{Map, Value, json};

use crate::core::config::ConfigKey;

use super::spec::ConfigFieldSpecKind;
use super::value::{ConfigValue, MergeMethod, Scalar};

/// JSON schema (draft-07) for a complete `odools.toml`.
pub fn config_json_schema() -> Value {
    let mut props = Map::new();
    props.insert("name".to_string(), json!({"type": "string"}));
    props.insert(
        "extends".to_string(),
        json!({"type": "string", "description": "Name of another profile to inherit from"}),
    );
    for &key in ConfigKey::all() {
        props.insert(key.as_str().to_string(), field_schema(key));
    }

    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Odoo LS configuration",
        "type": "object",
        "properties": {
            "config": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": Value::Object(props),
                }
            }
        }
    })
}

fn field_schema(key: ConfigKey) -> Value {
    let mut schema = match key.kind() {
        ConfigFieldSpecKind::Bool => json!({"type": "boolean"}),
        // `auto_refresh_delay` is clamped to [1000, 15000] by its runtime accessor;
        // advertise that range here so the schema matches the effective bounds.
        ConfigFieldSpecKind::U64 if key == ConfigKey::AutoRefreshDelay => {
            json!({"type": "integer", "minimum": 1000, "maximum": 15000})
        }
        ConfigFieldSpecKind::U64 => json!({"type": "integer", "minimum": 0}),
        ConfigFieldSpecKind::Str => json!({"type": "string"}),
        ConfigFieldSpecKind::DiagImportsMode => json!({"enum": ["none", "only_odoo", "all"]}),
        ConfigFieldSpecKind::MergeMethod => json!({"enum": MergeMethod::VALUES}),
        ConfigFieldSpecKind::StrList => json!({"type": "array", "items": {"type": "string"}}),
        ConfigFieldSpecKind::DiagSettings => json!({
            "type": "object",
            "additionalProperties": {"enum": ["Error", "Warning", "Info", "Hint", "Disabled"]}
        }),
        ConfigFieldSpecKind::DiagFilters => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "paths": {"type": "array", "items": {"type": "string"}},
                    "codes": {"type": "array", "items": {"type": "string"}},
                    "types": {"type": "array", "items": {"enum": ["Error", "Warning", "Info", "Hint"]}},
                    "path_type": {"enum": ["in", "notin"]}
                }
            }
        }),
    };

    // Surface the compile-time default, if any.
    if let Value::Object(obj) = &mut schema
        && let Some(ConfigValue::Scalar(s)) = key.default_value().as_ref()
    {
        let default = match s.value() {
            Scalar::Bool(b) => json!(b),
            Scalar::U64(n) => json!(n),
            Scalar::Str(s) => json!(s),
        };
        obj.insert("default".to_string(), default);
    }
    schema
}
