//! Version handling for the configuration pipeline, in two stages:
//!  - `expand_version_profiles` — **set-level**, runs before `extends`: resolves
//!    `${detectVersion}` (mutates `$version`/`$base`) and `${splitVersion}`
//!    (marks the parent abstract and spawns a child profile per version dir).
//!  - `resolve_version_value` — **per-field** hook on `$version`, runs after
//!    `extends`: turns a `$version` *path* (a dir or `__manifest__.py`) into a
//!    version string.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use ruff_python_ast::{Expr, Mod};
use ruff_python_parser::{Mode, ParseOptions};
use tracing::error;

use crate::S;
use crate::core::config::ConfigKey;
use crate::utils::{HashMap, HashSet, PathSanitizer};

use super::paths::{SymlinkPolicy, fill_template_path, resolve_path};
use super::value::{PipelineCtx, Profile, ProfileSet, config_dir, default_sources};

static VERSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^(\D+~)?\d+\.\d+$"#).unwrap());

const DETECT_VERSION: &str = "${detectVersion}";
const SPLIT_VERSION: &str = "${splitVersion}";

// ---------------------------------------------------------------------------
// Small scalar helpers
// ---------------------------------------------------------------------------

fn read_scalar(profile: &Profile, key: ConfigKey) -> Option<(String, HashSet<String>)> {
    let s = profile.scalar_str(key)?;
    Some((s.value, s.sources))
}

// ---------------------------------------------------------------------------
// ResolveVersionValue stage (per-field hook on $version)
// ---------------------------------------------------------------------------

fn parse_manifest_version(contents: &str) -> Option<String> {
    let parsed = ruff_python_parser::parse_unchecked(contents, ParseOptions::from(Mode::Module));
    if !parsed.errors().is_empty() {
        return None;
    }
    let Mod::Module(module) = parsed.into_syntax() else {
        return None;
    };
    // A manifest is conventionally a single dict literal, but tolerate extra
    // top-level statements (e.g. a `from __future__` import or an assignment):
    // use the first top-level dict expression instead of requiring exactly one
    // statement.
    let dict_expr = module
        .body
        .iter()
        .filter_map(|stmt| stmt.as_expr_stmt())
        .find_map(|expr| expr.value.as_dict_expr())?;
    for item in dict_expr.items.iter() {
        if !matches!(item.key.as_ref(), Some(Expr::StringLiteral(expr)) if expr.value.to_str() == "version")
        {
            continue;
        }
        if let Some(value_expr) = item.value.as_string_literal_expr() {
            return Some(value_expr.value.to_string());
        }
    }
    None
}

/// Resolve a `$version` value that is a path into a version string. Plain
/// version strings (and anything that doesn't resolve to a path) pass through.
/// A value that *does* resolve to a path but from which no version can be
/// extracted (e.g. a manifest with no parseable `version`) is an error: we must
/// not silently propagate the raw path string as if it were a version.
fn process_version(
    value: &str,
    sources: &HashSet<String>,
    ctx: &PipelineCtx,
) -> Result<String, String> {
    let dir = config_dir(sources);
    // Expand templates, then make it absolute + canonical. A value that is not a
    // (resolvable, existing) path is treated as a plain version string.
    let resolved = match fill_template_path(ctx.unique_ws_folders, ctx.current_ws, value, HashMap::default())
        .ok()
        .and_then(|filled| resolve_path(&filled, &dir, SymlinkPolicy::Resolve).ok())
    {
        Some(resolved) => resolved,
        None => return Ok(value.to_string()),
    };

    let path = PathBuf::from(&resolved);
    if path.is_file()
        && path
            .file_name()
            .is_some_and(|f| f.to_string_lossy() == "__manifest__.py")
    {
        let contents = fs::read_to_string(&path).map_err(|err| {
            format!("Failed to read manifest {path:?} for version inference: {err}")
        })?;
        // `parse_manifest_version` returns the already-unquoted string content.
        let raw = parse_manifest_version(&contents)
            .ok_or_else(|| format!("Could not determine a version from manifest {path:?}"))?;
        let mut parts = raw.split('.');
        return match (parts.next(), parts.next()) {
            (Some(major), Some(minor)) => Ok(format!("{major}.{minor}")),
            _ => Err(format!(
                "Manifest {path:?} has a malformed version '{raw}' (expected 'major.minor')"
            )),
        };
    }

    // `resolved` is already canonical; its last component is the version dir.
    match path.components().next_back() {
        Some(suffix) => Ok(suffix.as_os_str().to_string_lossy().to_string()),
        None => Err(format!("Could not extract a version from path {path:?}")),
    }
}

/// `ResolveVersionValue`-stage hook attached to `$version`.
pub(super) fn resolve_version_value(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    let Some((value, sources)) = read_scalar(profile, key) else {
        return Ok(());
    };
    match process_version(&value, &sources, ctx) {
        Ok(resolved) => profile.set_scalar_str(key, resolved, sources),
        Err(reason) => {
            profile.values.remove(&key);
            profile.add_rejected(key, value, sources, reason);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ExpandVersionProfiles stage (set-level, before extends)
// ---------------------------------------------------------------------------

fn path_ends_with(value: &str, token: &str) -> bool {
    PathBuf::from(value)
        .components()
        .next_back()
        .map(|c| c.as_os_str().to_string_lossy() == token)
        .unwrap_or(false)
}

/// `$base` ending in `${detectVersion}`: infer `$version` from the workspace
/// path's component just past the base prefix, and rewrite `$base` to point at
/// that version.
fn resolve_detect_version(profile: &mut Profile, ws_path: &Path) -> Result<(), String> {
    let Some((base_value, base_sources)) = read_scalar(profile, ConfigKey::Base) else {
        return Ok(());
    };
    if !path_ends_with(&base_value, DETECT_VERSION) {
        return Ok(());
    }
    let base_path = PathBuf::from(&base_value);
    let Some(prefix) = base_path.parent().map(PathBuf::from) else {
        return Err(S!("\"$base\" must be a valid path with a parent directory"));
    };
    let abs = prefix.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize base path: {} ({})",
            prefix.display(),
            e
        )
    })?;
    let prefix = PathBuf::from(abs.sanitize());
    let prefix_components: Vec<_> = prefix.components().collect();
    let ws_components: Vec<_> = ws_path.components().collect();

    if ws_components.len() <= prefix_components.len()
        || ws_components[..prefix_components.len()] != prefix_components[..]
    {
        return Err(S!("\"$base\" does not match the current workspace folder"));
    }
    let version = ws_components[prefix_components.len()]
        .as_os_str()
        .to_string_lossy()
        .to_string();
    if version.is_empty() {
        return Err(S!(
            "Could not extract version from workspace path using \"$base\""
        ));
    }

    profile.set_scalar_str(ConfigKey::Version, version.clone(), default_sources());
    profile.set_scalar_str(
        ConfigKey::Base,
        prefix.join(&version).sanitize(),
        base_sources,
    );
    Ok(())
}

/// `$version` ending in `${splitVersion}`: mark the profile abstract and spawn a
/// child profile for each version-named directory under the resolved parent.
fn resolve_split_version(profile: &mut Profile, ctx: &PipelineCtx) -> Result<Vec<Profile>, String> {
    let Some((version_value, version_sources)) = read_scalar(profile, ConfigKey::Version) else {
        return Ok(Vec::new());
    };
    if !path_ends_with(&version_value, SPLIT_VERSION) {
        return Ok(Vec::new());
    }
    profile.abstract_ = true;

    let Some(parent) = PathBuf::from(&version_value).parent().map(PathBuf::from) else {
        return Ok(Vec::new());
    };
    let dir = config_dir(&version_sources);
    let parent_resolved = match fill_template_path(ctx.unique_ws_folders, ctx.current_ws, &parent.sanitize(), HashMap::default())
        .and_then(|filled| resolve_path(&filled, &dir, SymlinkPolicy::Resolve).map_err(|e| e.to_string()))
    {
        Ok(p) => PathBuf::from(p),
        Err(err) => {
            error!(
                "Failed to resolve ${{splitVersion}} parent for {:?}: {}",
                version_value, err
            );
            return Ok(Vec::new());
        }
    };

    let mut spawned = Vec::new();
    // If the parent dir can't be read, spawn nothing (the directory may not
    // exist yet); per-entry read errors are likewise skipped via `flatten`.
    let entries = match fs::read_dir(&parent_resolved) {
        Ok(entries) => entries,
        Err(err) => {
            error!("Failed to read ${{splitVersion}} parent {parent_resolved:?}: {err}");
            return Ok(Vec::new());
        }
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir_name = entry.file_name();
        let dir_name = dir_name.to_string_lossy();
        if !VERSION_REGEX.is_match(&dir_name) {
            continue;
        }
        let mut child = profile.clone();
        child.name = format!("{}-{}", profile.name, dir_name);
        child.abstract_ = false;
        child.extends = Some(profile.name.clone());
        child.set_scalar_str(ConfigKey::Version, dir_name.to_string(), default_sources());
        spawned.push(child);
    }
    Ok(spawned)
}

/// Set-level driver: run detect/split over every profile, then add the spawned
/// children. Workspace-only (needs the workspace path).
pub(super) fn expand_version_profiles(
    set: &mut ProfileSet,
    ws: &(String, String),
    unique_ws: &HashMap<String, String>,
) -> Result<(), String> {
    let (_, ws_path) = ws;
    let ctx = PipelineCtx {
        unique_ws_folders: unique_ws,
        current_ws: Some(ws),
    };
    let mut spawned = Vec::new();
    for profile in set.values_mut() {
        resolve_detect_version(profile, Path::new(ws_path))?;
        spawned.extend(resolve_split_version(profile, &ctx)?);
    }
    for child in spawned {
        set.insert(child.name.clone(), child);
    }
    Ok(())
}
