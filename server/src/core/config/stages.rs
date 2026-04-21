//! Per-field stage hooks — the decoupled implementations referenced by
//! `FieldSpec` rows via `.stage(Stage, fn)`.
//!
//! Each hook operates on one field of one profile during a pipeline stage.

use std::path::{Path, PathBuf};

use crate::{
    core::config::ConfigKey,
    utils::{
        HashMap, HashSet, PathSanitizer, default_python_command, expand_language_code,
        fill_validate_path, has_template, is_addon_path, is_odoo_path, is_python_path,
    },
};

use super::value::{ConfigValue, PipelineCtx, Profile, Sourced, config_dir, group_sourced_iters};

/// List token that requests auto-detection of `addons_paths` (see `infer_addons`).
const AUTO_DETECT_ADDONS: &str = "$autoDetectAddons";

// ---------------------------------------------------------------------------
// PostMergeProcessing stage
// ---------------------------------------------------------------------------

/// `additional_languages`: expand each code to also include its base language
/// (`fr_BE` → `fr_BE`, `fr`), then regroup so a shared base unions its sources.
pub(super) fn expand_languages(
    profile: &mut Profile,
    key: ConfigKey,
    _ctx: &PipelineCtx,
) -> Result<(), String> {
    let Some(ConfigValue::List(items)) = profile.values.get(&key) else {
        return Ok(());
    };
    let expanded = items.iter().flat_map(|item| {
        let sources = item.sources();
        expand_language_code(item.value())
            .map(move |code| Sourced::with_sources(code, sources.clone()))
    });
    let regrouped: Vec<Sourced<String>> = group_sourced_iters(expanded).collect();
    profile.values.insert(key, ConfigValue::List(regrouped));
    Ok(())
}

/// `python_path` has no static default: when unset, fall back to the detected
/// `python`/`python3` command. When none is found, leave it unset.
pub(super) fn default_python_path(
    profile: &mut Profile,
    key: ConfigKey,
    _ctx: &PipelineCtx,
) -> Result<(), String> {
    if profile.get(key).is_none() {
        let cmd = default_python_command();
        if !cmd.is_empty() {
            profile.values.insert(key, ConfigValue::str(cmd));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ResolvePaths stage
// ---------------------------------------------------------------------------
//
// Path hooks resolve `${...}` templates and relative paths against the config
// file's directory, validate with a per-field predicate, and canonicalize.
// Invalid values are dropped.

/// `$version` / `$base` values feed `${version}` / `${base}` template vars.
fn var_map(profile: &Profile) -> HashMap<String, String> {
    let mut map = HashMap::default();
    if let Some(v) = profile
        .get(ConfigKey::Version)
        .and_then(ConfigValue::as_str)
    {
        map.insert("version".to_string(), v.to_string());
    }
    if let Some(b) = profile.get(ConfigKey::Base).and_then(ConfigValue::as_str) {
        map.insert("base".to_string(), b.to_string());
    }
    map
}

fn canonicalize_value<F: Fn(&str) -> bool>(
    sourced: &Sourced<String>,
    ctx: &PipelineCtx,
    predicate: F,
    vars: HashMap<String, String>,
) -> Result<Sourced<String>, String> {
    let dir = config_dir(sourced.sources());
    let make = |value: String| Sourced::with_sources(value, sourced.sources().clone());

    if has_template(sourced.value()) {
        let filled = fill_validate_path(
            ctx.unique_ws_folders,
            ctx.current_ws,
            sourced.value(),
            &predicate,
            vars,
            &dir,
        )?;
        let canon = std::fs::canonicalize(PathBuf::from(&filled))
            .map_err(|e| e.to_string())?
            .sanitize();
        return Ok(make(canon));
    }

    let mut path = PathBuf::from(sourced.value());
    if path.is_relative() {
        path = dir.join(sourced.value());
    }
    let canon = std::fs::canonicalize(path)
        .map_err(|e| e.to_string())?
        .sanitize();
    if !predicate(&canon) {
        return Err(format!(
            "path '{canon}' does not satisfy the required conditions"
        ));
    }
    Ok(make(canon))
}

fn canonicalize_scalar<F: Fn(&str) -> bool + Copy>(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
    predicate: F,
) -> Result<(), String> {
    let Some(sourced) = profile.scalar_str(key) else {
        return Ok(());
    };
    let vars = var_map(profile);
    match canonicalize_value(&sourced, ctx, predicate, vars) {
        Ok(c) => profile.set_scalar_str(key, c.value, c.sources),
        // Invalid: drop from the runtime values (so it falls back to unset rather
        // than using a bad path), but record it so the config panel surfaces the
        // failure — silently discarding an explicit setting reads as "the LSP
        // ignored my config".
        Err(err) => {
            profile.values.remove(&key);
            profile.add_rejected(
                key,
                sourced.value,
                sourced.sources,
                format!("could not be resolved: {err}"),
            );
        }
    }
    Ok(())
}

fn canonicalize_list<F: Fn(&str) -> bool + Copy>(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
    predicate: F,
) -> Result<(), String> {
    let Some(ConfigValue::List(items)) = profile.get(key) else {
        return Ok(());
    };
    let items = items.clone();
    let vars = var_map(profile);
    let mut out = Vec::new();
    for item in &items {
        // `$autoDetectAddons` is an inference token, handled in the
        // `AutoDetectPaths` stage — leave it untouched here.
        if item.value() == AUTO_DETECT_ADDONS {
            out.push(item.clone());
            continue;
        }
        match canonicalize_value(item, ctx, predicate, vars.clone()) {
            Ok(canon) => out.push(canon),
            // Capture the invalid entry for the panel instead of dropping it.
            Err(err) => profile.add_rejected(
                key,
                item.value().clone(),
                item.sources().clone(),
                format!("could not be resolved: {err}"),
            ),
        }
    }
    let grouped: Vec<Sourced<String>> = group_sourced_iters(out).collect();
    profile.values.insert(key, ConfigValue::List(grouped));
    Ok(())
}

pub(super) fn canon_odoo_path(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    canonicalize_scalar(profile, key, ctx, is_odoo_path)
}

pub(super) fn canon_stdlib(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    canonicalize_scalar(profile, key, ctx, |p: &str| Path::new(p).is_dir())
}

/// `python_path` may be a bare command (e.g. `python3`); if it already looks
/// like one, keep it verbatim — otherwise resolve it as a path.
pub(super) fn canon_python_path(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    if let Some(v) = profile.get(key).and_then(ConfigValue::as_str)
        && is_python_path(v)
    {
        return Ok(());
    }
    canonicalize_scalar(profile, key, ctx, is_python_path)
}

pub(super) fn canon_addons_paths(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    canonicalize_list(profile, key, ctx, is_addon_path)
}

pub(super) fn canon_stubs(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    canonicalize_list(profile, key, ctx, |p: &str| Path::new(p).is_dir())
}

/// `ResolvePaths`-stage hook for `diagnostic_filters`: expand `${...}`
/// template/var references in each filter's glob patterns (relative to the
/// filter's config file). Invalid patterns are dropped and noted on the filter.
pub(super) fn expand_filter_patterns(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    let vars = var_map(profile);
    let Some(ConfigValue::DiagFilters(filters)) = profile.values.get_mut(&key) else {
        return Ok(());
    };
    for filter in filters.iter_mut() {
        let dir = config_dir(filter.sources());
        let mut info_lines = vec![];
        let new_paths: Vec<glob::Pattern> = filter
            .value
            .paths
            .iter()
            .filter_map(|pattern| {
                let raw = pattern.to_string();
                let result = fill_validate_path(
                    ctx.unique_ws_folders,
                    ctx.current_ws,
                    &raw,
                    |_: &str| true,
                    vars.clone(),
                    &dir,
                )
                .and_then(|p| glob::Pattern::new(&p).map_err(|e| e.to_string()));
                match result {
                    Ok(p) => Some(p),
                    Err(err) => {
                        info_lines.push(format!("Failed to process pattern '{raw}': {err}"));
                        None
                    }
                }
            })
            .collect();
        filter.value.paths = new_paths;
        filter.info.push_str(&info_lines.join("\n"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AutoDetectPaths stage
// ---------------------------------------------------------------------------

/// Directory names never worth descending into during auto-detection: they are
/// never a match (so prune-on-hit wouldn't stop the scan from walking them) yet
/// can be enormous.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "__pycache__",
    "target",
    "dist",
    "build",
    "venv",
    ".venv",
    "site-packages",
];

/// Hard cap on how deep auto-detection descends below a workspace folder, so a
/// large/monorepo tree can't make startup walk every directory (addon roots
/// live near the top).
const MAX_SCAN_DEPTH: usize = 10;

/// Recursively scan descendant directories of a workspace folder, invoking
/// `callback` with each directory's sanitized path and a
/// `$workspaceFolder:<name>/<rel>` source tag.
///
/// The callback drives descent: `Ok(true)` marks a match and prunes it (its
/// children aren't roots); `Ok(false)` keeps descending. Hidden dirs and
/// `SKIP_DIRS` are skipped; symlinks are not followed, so cycles can't loop. An
/// explicit work-stack (not recursion) keeps a deep tree from overflowing the
/// call stack.
fn scan_workspace_children<F>(ws_path: &str, ws_name: &str, mut callback: F) -> Result<(), String>
where
    F: FnMut(&str, String) -> Result<bool, String>,
{
    // (directory to scan, its `/`-prefixed path relative to the workspace root,
    // its depth below the workspace root).
    let mut stack: Vec<(String, String, usize)> = vec![(ws_path.to_string(), String::new(), 0)];
    while let Some((dir, rel, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for dir_entry in entries.flatten() {
            if !dir_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = dir_entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let child_path = dir_entry.path().sanitize();
            let child_rel = format!("{rel}/{name}");
            let source = format!("$workspaceFolder:{ws_name}{child_rel}");
            // A match prunes this branch; otherwise queue it for descent, unless
            // we have hit the depth cap.
            if !callback(&child_path, source)? && depth + 1 < MAX_SCAN_DEPTH {
                stack.push((child_path, child_rel, depth + 1));
            }
        }
    }
    Ok(())
}

/// A directory that is itself an Odoo module (holds a `__manifest__.py`). Its
/// subdirectories are module internals, never further addon roots — used to
/// prune the recursive workspace scan.
fn is_module_dir(path: &str) -> bool {
    Path::new(path).join("__manifest__.py").is_file()
}

/// `AutoDetectPaths`-stage hook for `addons_paths`: when unset, or when it
/// contains the `$autoDetectAddons` token, add the current workspace folder and
/// its immediate addon subdirectories.
pub(super) fn infer_addons(
    profile: &mut Profile,
    key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    let infer = match profile.get(key) {
        None => true,
        Some(ConfigValue::List(items)) => items.iter().any(|i| i.value() == AUTO_DETECT_ADDONS),
        _ => false,
    };
    // Strip the token regardless (it's not a real path).
    if let Some(ConfigValue::List(items)) = profile.values.get_mut(&key) {
        items.retain(|i| i.value() != AUTO_DETECT_ADDONS);
    }
    if !infer {
        return Ok(());
    }
    let Some((ws_name, ws_path)) = ctx.current_ws else {
        return Ok(());
    };
    let ws_path = PathBuf::from(ws_path).sanitize();

    let mut additions: Vec<Sourced<String>> = Vec::new();
    if is_addon_path(&ws_path) {
        additions.push(Sourced::new(
            ws_path.clone(),
            format!("$workspaceFolder:{ws_name}"),
        ));
    }
    scan_workspace_children(&ws_path, ws_name, |child, source| {
        // A module's internals are never addon roots — prune without a read_dir.
        if is_module_dir(child) {
            return Ok(true);
        }
        if is_addon_path(child) {
            additions.push(Sourced::new(child.to_string(), source));
            // An addons dir's children are modules — stop descending here.
            return Ok(true);
        }
        Ok(false)
    })?;

    let had_list = matches!(profile.get(key), Some(ConfigValue::List(_)));
    let existing = match profile.values.remove(&key) {
        Some(ConfigValue::List(v)) => v,
        _ => Vec::new(),
    };
    let combined: Vec<Sourced<String>> =
        group_sourced_iters(existing.into_iter().chain(additions)).collect();
    if !combined.is_empty() {
        profile.values.insert(key, ConfigValue::List(combined));
    } else if had_list {
        // Detection found nothing, but the user explicitly wrote a list (e.g.
        // `addons_paths = ["$autoDetectAddons"]`). Re-insert an empty list rather
        // than dropping the key: with override merge semantics, an absent key
        // inherits the parent's list, whereas an explicit empty list must keep
        // overriding it to empty.
        profile.values.insert(key, ConfigValue::List(Vec::new()));
    }
    Ok(())
}

/// Global inference (after cross-workspace merge): when `odoo_path` is unset,
/// detect it from a workspace folder root, then from immediate children.
/// Detecting more than one is an error.
pub(super) fn infer_odoo_path(
    profile: &mut Profile,
    _key: ConfigKey,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    // Iterate workspace folders in a stable order: `unique_ws` is a `HashMap`, and
    // the conflict error names whichever valid root is hit first — so without
    // sorting the diagnostic text would vary run-to-run.
    let mut ws_sorted: Vec<(&String, &String)> = ctx.unique_ws_folders.iter().collect();
    ws_sorted.sort();
    if profile.get(ConfigKey::OdooPath).is_some() {
        return Ok(());
    }
    // First pass: workspace folder roots.
    for (name, path) in &ws_sorted {
        try_set_odoo_path(profile, path, format!("$workspaceFolder:{name}"))?;
    }
    // Second pass: immediate children, only if no root matched.
    // Only check one level deep, to avoid false positives from nested workspaces.
    if profile.get(ConfigKey::OdooPath).is_none() {
        for (name, path) in &ws_sorted {
            scan_workspace_children(path, name, |child, source| {
                try_set_odoo_path(profile, child, source)?;
                Ok(true)
            })?;
        }
    }
    Ok(())
}

fn try_set_odoo_path(profile: &mut Profile, path: &str, source: String) -> Result<(), String> {
    if !is_odoo_path(path) {
        return Ok(());
    }
    if let Some(ConfigValue::Scalar(prev)) = profile.get(ConfigKey::OdooPath) {
        let prev_sources: Vec<String> = prev.sources().iter().cloned().collect();
        return Err(format!(
            "More than one workspace folder or subfolder is a valid odoo_path.\n\
             Please set the odoo_path in the config file.\n\
             Conflicting path: '{}', previously set from '{}'",
            path,
            prev_sources.join(", ")
        ));
    }
    profile.set_scalar_str(
        ConfigKey::OdooPath,
        path.to_string(),
        HashSet::from_iter([source]),
    );
    Ok(())
}
