use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use crate::constants::CONFIG_WIKI_URL;
use crate::core::config::ConfigKey;
use crate::utils::HashSet;

use crate::utils::HashMap;

use super::value::{ConfigValue, ProfileSet, Scalar, Sourced, is_fs_path};

// ---------------------------------------------------------------------------
// ConfigView — render view of the resolved profiles (for the IDE config panel).
// Built from the resolved `Profile`s, preserving per-value provenance.
// ---------------------------------------------------------------------------

/// Field display order in the panel and serialized output.
fn display_order() -> Vec<&'static str> {
    let mut order = vec!["name", "extends", "abstract"];
    order.extend(ConfigKey::all().iter().map(|k| k.as_str()));
    order
}

/// One `[[config]]` entry as resolved, with provenance retained for display.
#[derive(Debug, Clone)]
pub struct ProfileView {
    pub name: String,
    extends: Option<String>,
    abstract_: bool,
    values: super::value::ValuesMap,
    /// Settings the user gave explicitly but which failed resolution — shown in
    /// the panel (with their failure note) even though they are not used.
    rejected: HashMap<ConfigKey, Vec<Sourced<String>>>,
}

impl ProfileView {
    pub fn is_abstract(&self) -> bool {
        self.abstract_
    }
    /// Provenance of a scalar field, if present.
    pub fn scalar_sources(&self, key: ConfigKey) -> Option<&HashSet<String>> {
        match self.values.get(&key)? {
            ConfigValue::Scalar(s) => Some(s.sources()),
            _ => None,
        }
    }
    /// String value of a scalar field, if present and string-typed.
    pub fn str_value(&self, key: ConfigKey) -> Option<&str> {
        self.values.get(&key).and_then(ConfigValue::as_str)
    }
    /// Elements (value + provenance) of a list field; empty if absent.
    pub fn list(&self, key: ConfigKey) -> &[Sourced<String>] {
        match self.values.get(&key) {
            Some(ConfigValue::List(l)) => l,
            _ => &[],
        }
    }

    /// `(level, message, profile)` for this profile's rejected values, for the
    /// client's status-bar diagnostic. Level 1 if a fallback value survived,
    /// 2 if none did. Profile is separate from the message so the client can
    /// scope display to whichever profile is selected.
    fn diagnostic_messages(&self) -> impl Iterator<Item = (u8, String, &str)> + '_ {
        self.rejected.iter().flat_map(move |(key, rejected)| {
            let effective = self.values.get(key);
            let (level, outcome) = match effective {
                // A list can survive the rejection with zero valid entries left —
                // as bad as the key being absent, so treat it the same way.
                Some(ConfigValue::List(items)) if items.is_empty() => {
                    (2, "no valid entries remain for it".to_string())
                }
                Some(v) => (1, format!("using {} instead", describe_value(v))),
                None => match key.default_value() {
                    Some(d) => (
                        1,
                        format!("falling back to the default ({})", describe_value(&d)),
                    ),
                    None => (2, "no value is set for it".to_string()),
                },
            };
            rejected.iter().map(move |r| {
                (
                    level,
                    format!(
                        "Config profile '{}': {} = '{}' in {} was rejected ({}); {}.",
                        self.name,
                        key.as_str(),
                        r.value(),
                        sources_note(r.sources()),
                        r.info,
                        outcome,
                    ),
                    self.name.as_str(),
                )
            })
        })
    }

    /// JSON shape consumed by the renderer and by serialization:
    /// scalars → `{value, sources, info}`, lists → `[{value, sources, info}]`.
    fn to_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("name".to_string(), json!(self.name));
        obj.insert(
            "extends".to_string(),
            json!(self.extends.clone().unwrap_or_default()),
        );
        obj.insert("abstract".to_string(), json!(self.abstract_));
        for &key in ConfigKey::all() {
            let value = self.values.get(&key);
            let rejected = self.rejected.get(&key).filter(|v| !v.is_empty());
            let field = match (value, rejected) {
                (None, None) => continue,
                (Some(v), None) => value_to_json(v),
                // A list with some invalid items: render the valid items, then the
                // rejected ones (each carrying its failure note) in one array.
                (Some(ConfigValue::List(items)), Some(rej)) => Value::Array(
                    items
                        .iter()
                        .chain(rej.iter())
                        .map(sourced_string_json)
                        .collect(),
                ),
                // A scalar value is present but the user's explicit value was
                // rejected — a fallback default or inference filled it in. Show the
                // effective value and fold the rejected one(s) into its note, so the
                // failure isn't hidden behind the inferred value.
                (Some(ConfigValue::Scalar(s)), Some(rej)) => {
                    let mut info = s.info.clone();
                    for r in rej {
                        if !info.is_empty() {
                            info.push_str("; ");
                        }
                        info.push_str(&format!("ignored '{}' ({})", r.value(), r.info));
                    }
                    json!({"value": scalar_json(s.value()), "sources": sources_json(s.sources()), "info": info})
                }
                // Other kinds never record rejections; render as-is.
                (Some(v), Some(_)) => value_to_json(v),
                // No valid value survived: surface the rejected value(s) with notes.
                (None, Some(rej)) if rej.len() == 1 => sourced_string_json(&rej[0]),
                (None, Some(rej)) => Value::Array(rej.iter().map(sourced_string_json).collect()),
            };
            obj.insert(key.as_str().to_string(), field);
        }
        Value::Object(obj)
    }
}

fn sorted_sources(sources: &HashSet<String>) -> Vec<&String> {
    let mut v: Vec<&String> = sources.iter().collect();
    v.sort();
    v
}

fn sources_json(sources: &HashSet<String>) -> Value {
    json!(sorted_sources(sources))
}

/// Comma-joined, sorted source list for a plain-text diagnostic message.
fn sources_note(sources: &HashSet<String>) -> String {
    sorted_sources(sources)
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Short human description of a config value, for "using X instead" notes.
fn describe_value(value: &ConfigValue) -> String {
    match value {
        ConfigValue::Scalar(s) => format!("'{}'", scalar_display(s.value())),
        ConfigValue::List(items) => match items.len() {
            0 => "an empty list".to_string(),
            1 => "the 1 remaining valid entry".to_string(),
            n => format!("the {n} remaining valid entries"),
        },
        _ => "the existing value".to_string(),
    }
}

/// Plain (unquoted) display form of a scalar, for prose notes — unlike
/// `scalar_json`, a `Str` renders without surrounding JSON quotes.
fn scalar_display(value: &Scalar) -> String {
    match value {
        Scalar::Str(s) => s.clone(),
        other => scalar_json(other).to_string(),
    }
}

fn scalar_json(value: &Scalar) -> Value {
    match value {
        Scalar::Bool(b) => json!(b),
        Scalar::U64(n) => json!(n),
        Scalar::Str(s) => json!(s),
    }
}

/// The `{value, sources, info}` shape for a string-valued list item (or a
/// rejected entry, which is also just a string value with provenance + note).
fn sourced_string_json(s: &Sourced<String>) -> Value {
    json!({"value": s.value(), "sources": sources_json(s.sources()), "info": s.info})
}

fn value_to_json(value: &ConfigValue) -> Value {
    match value {
        ConfigValue::Scalar(s) => {
            json!({"value": scalar_json(s.value()), "sources": sources_json(s.sources()), "info": s.info})
        }
        ConfigValue::List(items) => Value::Array(items.iter().map(sourced_string_json).collect()),
        ConfigValue::DiagSettings(map) => {
            let mut obj = serde_json::Map::new();
            for (code, setting) in map {
                let val = serde_json::to_value(setting.value()).unwrap_or(Value::Null);
                obj.insert(
                    code.to_string(),
                    json!({"value": val, "sources": sources_json(setting.sources()), "info": setting.info}),
                );
            }
            Value::Object(obj)
        }
        ConfigValue::DiagFilters(filters) => Value::Array(
            filters
                .iter()
                .map(|f| {
                    let val = serde_json::to_value(f.value()).unwrap_or(Value::Null);
                    json!({"value": val, "sources": sources_json(f.sources()), "info": f.info})
                })
                .collect(),
        ),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfigView {
    pub configs: Vec<ProfileView>,
}

impl ConfigView {
    pub fn new() -> Self {
        Self::default()
    }

    /// A view holding a single profile (used to render one `[[config]]` entry).
    pub fn single(entry: ProfileView) -> Self {
        ConfigView {
            configs: vec![entry],
        }
    }

    /// The profiles held by this view, in resolution order.
    pub fn entries(&self) -> &[ProfileView] {
        &self.configs
    }

    /// `{level, message, profile}` entries for every rejected value across all
    /// profiles, sent as the `diagnostics` field of `$Odoo/setConfiguration` so
    /// the client can color the status bar (1 = warning, 2 = error) and scope
    /// the tooltip to whichever profile is actually selected.
    pub fn diagnostic_messages(&self) -> Vec<Value> {
        self.configs
            .iter()
            .flat_map(ProfileView::diagnostic_messages)
            .map(|(level, message, profile)| {
                json!({"level": level, "message": message, "profile": profile})
            })
            .collect()
    }

    pub(super) fn from_profiles(profiles: &ProfileSet) -> Self {
        let configs = profiles
            .values()
            .map(|p| ProfileView {
                name: p.name.clone(),
                extends: p.extends.clone(),
                abstract_: p.abstract_,
                values: p.values.clone(),
                rejected: p.rejected.clone(),
            })
            .collect();
        ConfigView { configs }
    }

    pub fn to_html_string(&self) -> String {
        let mut r = ConfigRenderer::new();
        r.write_header();

        let entry_htmls: Vec<String> = self
            .configs
            .iter()
            .map(|entry| r.render_entry(&entry.to_json()))
            .collect();

        r.html
            .push_str(&entry_htmls.join("<div class=\"toml-line-break\"></div>\n"));
        r.html.push_str("</div>\n");
        r.finish()
    }
}

impl Serialize for ConfigView {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let config: Vec<Value> = self.configs.iter().map(ProfileView::to_json).collect();
        json!({ "config": config }).serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// ConfigRenderer — structured HTML generation
// ---------------------------------------------------------------------------

struct ConfigRenderer {
    html: String,
}

impl ConfigRenderer {
    fn new() -> Self {
        Self {
            html: String::new(),
        }
    }

    fn write_header(&mut self) {
        self.html.push_str(CSS);
        self.html.push_str("<a class=\"config-wiki-link\" href=\"");
        self.html.push_str(CONFIG_WIKI_URL);
        self.html.push_str(
            "\" target=\"_blank\" rel=\"noopener\">Configuration file documentation &rarr;</a>\n",
        );
        self.html.push_str("<div class=\"toml-table\">\n");
    }

    fn render_entry(&self, entry_val: &serde_json::Value) -> String {
        let mut html = String::new();
        html.push_str("<div class=\"toml-row\"><div class=\"toml-left\"><b>[[config]]</b></div><div class=\"toml-right\"></div></div>\n");
        if let serde_json::Value::Object(map) = entry_val {
            for key in display_order() {
                if let Some(val) = map.get(key) {
                    html.push_str(&Self::render_field(key, val, 0));
                }
            }
        }
        html
    }

    fn finish(self) -> String {
        self.html
    }

    // --- Field rendering dispatch ---

    fn render_field(key: &str, value: &serde_json::Value, indent: usize) -> String {
        if Self::is_sourced(value) {
            Self::render_sourced_field(key, value, indent)
        } else if value.is_array() {
            Self::render_array(key, value, "", "", indent)
        } else if value.is_object() && !value.is_null() {
            Self::render_plain_object(key, value, indent)
        } else {
            Self::render_plain_scalar(key, value, indent)
        }
    }

    // --- Sourced field rendering ---

    fn render_sourced_field(key: &str, value: &serde_json::Value, indent: usize) -> String {
        let val = &value["value"];
        let sources_html = Self::render_sources(&value["sources"]);
        let info_html = Self::render_info(value);

        if val.is_object() && !val.is_null() {
            Self::render_sourced_object(key, val, &info_html)
        } else if val.is_array() {
            Self::render_array(key, val, &info_html, &sources_html, indent)
        } else {
            Self::render_sourced_scalar(key, val, &sources_html, &info_html, indent)
        }
    }

    fn render_sourced_scalar(
        key: &str,
        val: &serde_json::Value,
        sources: &str,
        info: &str,
        indent: usize,
    ) -> String {
        Self::row(
            &format!("{}{} = {}", " ".repeat(indent * 2), esc(key), esc_json(val)),
            &format!("{}{}", sources, info),
        )
    }

    /// Render an array field. `header_right` annotates the opening `[` row and
    /// `item_fallback` is the right-column text for non-sourced items (sourced
    /// items always carry their own sources/info). The plain and sourced callers
    /// differ only in these two strings.
    fn render_array(
        key: &str,
        val: &serde_json::Value,
        header_right: &str,
        item_fallback: &str,
        indent: usize,
    ) -> String {
        let mut rows = Self::row(&format!("{} = [", esc(key)), header_right);
        for item in val.as_array().unwrap() {
            let (item_val, right) = if Self::is_sourced(item) {
                let item_sources = Self::render_sources(&item["sources"]);
                let item_info = Self::render_info(item);
                (&item["value"], format!("{item_sources}{item_info}"))
            } else {
                (item, item_fallback.to_string())
            };
            rows.push_str(&Self::row(
                &format!("{}{},", " ".repeat((indent + 1) * 2), esc_json(item_val)),
                &right,
            ));
        }
        rows.push_str(&Self::row("]", ""));
        rows
    }

    fn render_sourced_object(key: &str, val: &serde_json::Value, info: &str) -> String {
        let mut rows = Self::row(&format!("{} = {{", esc(key)), info);
        for (k, v) in val.as_object().unwrap() {
            for line in Self::render_field(k, v, 1).lines() {
                rows.push_str(&format!(
                    "<div class=\"toml-row\"><div class=\"toml-left\">  {}</div></div>\n",
                    line
                ));
            }
        }
        rows.push_str(&Self::row("}", ""));
        rows
    }

    // --- Plain (non-sourced) field rendering ---

    fn render_plain_object(key: &str, value: &serde_json::Value, _indent: usize) -> String {
        let mut rows = Self::row(&format!("{} = {{", esc(key)), "");
        for (k, v) in value.as_object().unwrap() {
            for line in Self::render_field(k, v, 1).lines() {
                rows.push_str(line);
            }
        }
        rows.push_str(&Self::row("}", ""));
        rows
    }

    fn render_plain_scalar(key: &str, value: &serde_json::Value, indent: usize) -> String {
        Self::row(
            &format!("{}{} = {}", " ".repeat(indent * 2), esc(key), esc_json(value)),
            "",
        )
    }

    // --- Helpers ---

    fn row(left: &str, right: &str) -> String {
        format!(
            "<div class=\"toml-row\"><div class=\"toml-left\">{}</div><div class=\"toml-right\">{}</div></div>\n",
            left, right
        )
    }

    fn is_sourced(val: &serde_json::Value) -> bool {
        val.get("value").is_some()
            && val.get("sources").is_some()
            && val.get("sources").unwrap().is_array()
    }

    fn render_source(source: &str) -> String {
        if is_fs_path(source) {
            format!(
                "<a href=\"file:///{}\">{}</a>",
                esc(&source.replace('\\', "/")),
                esc(source)
            )
        } else {
            esc(source)
        }
    }

    fn render_sources(sources: &serde_json::Value) -> String {
        sources
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str())
                    .map(Self::render_source)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    fn render_info(value: &serde_json::Value) -> String {
        value
            .get("info")
            .and_then(|info| info.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| format!("<span class=\"toml-info\">⚠ {}</span>", esc(s)))
            .unwrap_or_default()
    }
}

/// Escape text for safe interpolation into the config-panel HTML. Config values
/// (paths, regexes, info notes) are user-controlled and flow into a VS Code
/// webview, so every dynamic fragment must be escaped to avoid HTML/script
/// injection.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape a JSON scalar's display form (e.g. `"foo"`, `42`, `true`) for HTML.
fn esc_json(v: &serde_json::Value) -> String {
    esc(&v.to_string())
}

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"<style>
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
    text-align: right;
  }
    .toml-info {
        display: block;
        text-align: left;
        color: #c00;
        font-size: 0.9em;
        margin-top: 2px;
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
"#;
