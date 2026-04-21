//! The configuration pipeline entry point: walk workspace folders + config
//! file, parse into `Profile`s, run per-source stages (version, path
//! canonicalization, inference), merge across sources (dir-walk child-wins,
//! profile `extends`, cross-folder agree-or-error), fill defaults, and produce
//! the runtime `ConfigEntry` map plus the `ConfigView` render view.

use std::path::PathBuf;

use crate::core::config::{ConfigKey, ConfigMap};
use crate::threads::SessionInfo;
use crate::utils::{HashMap, HashSet};

use super::parse::parse_file;
use super::render::ConfigView;
use super::runtime::ConfigEntry;
use super::spec::{MergeRule, Stage, registry};
use std::str::FromStr;

use super::value::DEFAULT_PROFILE_NAME;
use super::value::{
    ConfigValue, MergeMethod, PipelineCtx, Profile, ProfileSet, group_sourced_iters,
    merge_sourced_diagnostic_setting_map,
};

const CONFIG_FILE: &str = "odools.toml";

/// Run every field's hooks registered for `stage` over each profile.
fn run_stage_hooks(
    stage: Stage,
    profiles: &mut ProfileSet,
    ctx: &PipelineCtx,
) -> Result<(), String> {
    for profile in profiles.values_mut() {
        // Abstract profiles (e.g. a ${splitVersion} parent) are templates, not
        // usable configs — skip their per-field hooks.
        if profile.abstract_ {
            continue;
        }
        for &key in ConfigKey::all() {
            let Some(spec) = registry().get(&key) else {
                continue;
            };
            for hook in spec.stages.iter().filter(|h| h.stage == stage) {
                (hook.run)(profile, key, ctx)?;
            }
        }
    }
    Ok(())
}

/// Resolve the full configuration: the runtime `ConfigEntry` map (one per
/// profile) plus the `ConfigView` render view (provenance retained).
pub(super) fn resolve(session: &mut SessionInfo) -> Result<(ConfigMap, ConfigView), String> {
    let profiles = build_profiles(session)?;
    let config_view = ConfigView::from_profiles(&profiles);
    let map = profiles
        .into_iter()
        .map(|(name, profile)| (name, ConfigEntry::from_profile(profile)))
        .collect();
    Ok((map, config_view))
}

fn build_profiles(session: &mut SessionInfo) -> Result<ProfileSet, String> {
    let (ws_folders, unique_ws): (Vec<(String, String)>, HashMap<String, String>) = {
        let fm = session.sync_odoo.get_file_mgr();
        let fm = fm.borrow();
        (
            fm.get_processed_workspace_folders().into_iter().collect(),
            fm.get_unique_workspace_folders(),
        )
    };

    // Resolve each source (config file, then each workspace folder)
    let mut profile_sets: Vec<ProfileSet> = Vec::new();
    if let Some(path) = session.sync_odoo.config_path.as_ref() {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(format!("Config file not found: {}", path.display()));
        }
        profile_sets.push(resolve_file(&path, &unique_ws)?);
    }
    for ws in &ws_folders {
        profile_sets.push(resolve_workspace(ws, &unique_ws)?);
    }

    // Merge across sources: scalars must agree or error
    let mut acc: ProfileSet = HashMap::default();
    for profile_set in profile_sets {
        acc = join_profile_sets(&acc, &profile_set, merge_ws_profile)?;
    }

    // Fill in any missing fields with their default values (from the spec).
    fill_defaults(&mut acc);

    let ctx = PipelineCtx {
        unique_ws_folders: &session
            .sync_odoo
            .get_file_mgr()
            .borrow()
            .get_unique_workspace_folders(),
        current_ws: None, // Global so we do not have a specific ws
    };
    run_stage_hooks(Stage::PostMergeProcessing, &mut acc, &ctx)?;

    Ok(acc)
}

// ---------------------------------------------------------------------------
// Per-source resolution
// ---------------------------------------------------------------------------

/// A standalone config file: its profiles, then `extends` → version-value →
/// canonicalize (no current workspace; no `${detectVersion}`/`${splitVersion}`,
/// which require a workspace path).
fn resolve_file(
    path: &std::path::Path,
    unique_ws: &HashMap<String, String>,
) -> Result<ProfileSet, String> {
    // Seed a `default` profile so a config file with only named profiles (or none)
    // still yields a `default`, matching `resolve_workspace` and the headless case.
    let mut seed: ProfileSet = HashMap::default();
    seed.insert(
        DEFAULT_PROFILE_NAME.to_string(),
        Profile::new(DEFAULT_PROFILE_NAME),
    );
    let file_set = into_set(parse_file(path)?);
    let mut set = merge_sets_child_wins(&file_set, &seed);
    finalize_source(&mut set, unique_ws, None)?;
    Ok(set)
}

/// A workspace folder: walk from the folder up to the root, merging each
/// `odools.toml` (deeper wins), expand version profiles, then finalize.
fn resolve_workspace(
    ws: &(String, String),
    unique_ws: &HashMap<String, String>,
) -> Result<ProfileSet, String> {
    let mut acc: ProfileSet = HashMap::default();
    acc.insert(
        DEFAULT_PROFILE_NAME.to_string(),
        Profile::new(DEFAULT_PROFILE_NAME),
    );

    let (_, ws_path) = ws;
    let mut dir = PathBuf::from(ws_path);
    let mut visited: HashSet<PathBuf> = HashSet::default();
    loop {
        if !visited.insert(dir.clone()) {
            break;
        }
        let cfg = dir.join(CONFIG_FILE);
        if cfg.is_file() {
            let file_set = into_set(parse_file(&cfg)?);
            // `acc` is the deeper (already-merged) set → it is the child.
            acc = merge_sets_child_wins(&acc, &file_set);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }

    // Set-level, before extends: resolve ${detectVersion}, spawn ${splitVersion} children.
    super::version::expand_version_profiles(&mut acc, ws, unique_ws)?;
    finalize_source(&mut acc, unique_ws, Some(ws))?;
    Ok(acc)
}

/// Per-source stage sequence after parsing/merging files: `extends`, then the
/// `ResolveVersionValue` and `ResolvePaths` hook passes.
fn finalize_source(
    set: &mut ProfileSet,
    unique_ws: &HashMap<String, String>,
    current_ws: Option<&(String, String)>,
) -> Result<(), String> {
    apply_extends(set)?;
    let ctx = PipelineCtx {
        unique_ws_folders: unique_ws,
        current_ws,
    };
    run_stage_hooks(Stage::ResolveVersionValue, set, &ctx)?;
    run_stage_hooks(Stage::ResolvePaths, set, &ctx)?;
    // Addon inference is per-workspace (needs the current workspace folder).
    run_stage_hooks(Stage::AutoDetectPaths, set, &ctx)?;
    Ok(())
}

fn into_set(profiles: Vec<Profile>) -> ProfileSet {
    // Duplicate names within one file: last wins.
    profiles.into_iter().map(|p| (p.name.clone(), p)).collect()
}

// ---------------------------------------------------------------------------
// Merge: child-wins (dir-walk + extends)
// ---------------------------------------------------------------------------

fn keys_union(a: &Profile, b: &Profile) -> Vec<ConfigKey> {
    let mut keys: Vec<ConfigKey> = a.values.keys().copied().collect();
    for &k in b.values.keys() {
        if !a.values.contains_key(&k) {
            keys.push(k);
        }
    }
    keys
}

/// Merge a child profile over a parent, applying each field's registry.
fn merge_child_wins(child: &Profile, parent: &Profile) -> Profile {
    let mut result = Profile::new(child.name.clone());
    result.extends = child.extends.clone().or_else(|| parent.extends.clone());
    // `abstract_` is intrinsic to a profile and is NOT inherited via `extends`
    // (a concrete child of a ${splitVersion} parent stays concrete).
    result.abstract_ = child.abstract_;
    for key in keys_union(child, parent) {
        if let Some(value) = merge_value(key, child.get(key), parent.get(key), child) {
            result.values.insert(key, value);
        }
    }
    result
}

/// Combine two optional lists into one, grouping equal values and unioning
/// their sources (per-element provenance).
fn merge_lists(child: Option<&ConfigValue>, parent: Option<&ConfigValue>) -> Option<ConfigValue> {
    let child_list = child.and_then(ConfigValue::as_list);
    let parent_list = parent.and_then(ConfigValue::as_list);
    if child_list.is_none() && parent_list.is_none() {
        return None;
    }
    let all = child_list
        .into_iter()
        .flatten()
        .chain(parent_list.into_iter().flatten())
        .cloned();
    Some(ConfigValue::List(group_sourced_iters(all).collect()))
}

/// Resolve one field's value when `child` overrides `parent`, per its rule.
fn merge_value(
    key: ConfigKey,
    child: Option<&ConfigValue>,
    parent: Option<&ConfigValue>,
    child_profile: &Profile,
) -> Option<ConfigValue> {
    match registry().get(&key).map(|s| s.merge).unwrap_or_default() {
        MergeRule::ListAlwaysMerge => merge_lists(child, parent),
        MergeRule::ListControlledBy(ctrl) => {
            let method = child_profile
                .get(ctrl)
                .and_then(ConfigValue::as_str)
                .and_then(|s| MergeMethod::from_str(s).ok())
                .unwrap_or_default();
            match method {
                MergeMethod::Override => child.cloned(),
                MergeMethod::Merge => merge_lists(child, parent),
            }
        }
        MergeRule::DiagMap => {
            match (
                child.and_then(ConfigValue::as_diag_settings),
                parent.and_then(ConfigValue::as_diag_settings),
            ) {
                (None, None) => None,
                (Some(c), None) => Some(ConfigValue::DiagSettings(c.clone())),
                (None, Some(p)) => Some(ConfigValue::DiagSettings(p.clone())),
                (Some(c), Some(p)) => Some(ConfigValue::DiagSettings(
                    merge_sourced_diagnostic_setting_map(c, p),
                )),
            }
        }
        MergeRule::DiagFiltersConcat => {
            let child_filters = child.and_then(ConfigValue::as_diag_filters);
            let parent_filters = parent.and_then(ConfigValue::as_diag_filters);
            if child_filters.is_none() && parent_filters.is_none() {
                None
            } else {
                Some(ConfigValue::DiagFilters(
                    child_filters
                        .into_iter()
                        .flatten()
                        .chain(parent_filters.into_iter().flatten())
                        .cloned()
                        .collect(),
                ))
            }
        }
        MergeRule::ScalarChildWins => child.cloned().or_else(|| parent.cloned()),
    }
}

/// Outer-join two profile sets by name: a profile present in both is combined
/// with `combine`; one present on a single side passes through unchanged. The
/// two merge directions — child-wins (dir-walk + `extends`) and cross-workspace
/// (agree-or-error) — differ only in `combine`.
fn join_profile_sets<F>(
    a: &ProfileSet,
    b: &ProfileSet,
    mut combine: F,
) -> Result<ProfileSet, String>
where
    F: FnMut(&Profile, &Profile, &str) -> Result<Profile, String>,
{
    let names: HashSet<&String> = a.keys().chain(b.keys()).collect();
    let mut out = ProfileSet::default();
    for name in names {
        let merged = match (a.get(name), b.get(name)) {
            (Some(x), Some(y)) => combine(x, y, name)?,
            (Some(p), None) | (None, Some(p)) => p.clone(),
            (None, None) => unreachable!(),
        };
        out.insert(name.clone(), merged);
    }
    Ok(out)
}

/// Merge a child set over a parent set (dir-walk + `extends`): child wins.
fn merge_sets_child_wins(child: &ProfileSet, parent: &ProfileSet) -> ProfileSet {
    // `merge_child_wins` never fails, so the join is infallible here.
    join_profile_sets(child, parent, |c, p, _| Ok(merge_child_wins(c, p)))
        .expect("child-wins merge is infallible")
}

// ---------------------------------------------------------------------------
// extends resolution (topological; detects cycles and missing parents)
// ---------------------------------------------------------------------------

fn apply_extends(set: &mut ProfileSet) -> Result<(), String> {
    let keys: Vec<String> = set.keys().cloned().collect();

    // Order profiles by parent-chain length so a parent is merged before its
    // children. Walk each chain with a local visited set to detect cycles —
    // including a rootless cycle (A→B→A) that a root-seeded DFS would miss.
    let mut depth: HashMap<String, usize> = HashMap::default();
    for key in &keys {
        let mut seen: HashSet<String> = HashSet::from_iter([key.clone()]);
        let mut current = key.clone();
        let mut chain_len = 0;
        while let Some(parent) = set.get(&current).and_then(|p| p.extends.clone()) {
            if !set.contains_key(&parent) {
                return Err(format!(
                    "Profile '{}' extends non-existing profile '{}'",
                    current, parent
                ));
            }
            if !seen.insert(parent.clone()) {
                return Err("Circular dependency detected in profile extensions!".to_string());
            }
            chain_len += 1;
            current = parent;
        }
        depth.insert(key.clone(), chain_len);
    }

    let mut ordered = keys;
    ordered.sort_by_key(|k| depth[k]);

    for key in ordered {
        let Some(entry) = set.get(&key).cloned() else {
            continue;
        };
        let Some(parent) = entry.extends.as_ref().and_then(|p| set.get(p).cloned()) else {
            continue;
        };
        set.insert(key, merge_child_wins(&entry, &parent));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-workspace merge: scalars must agree or error
// ---------------------------------------------------------------------------

fn merge_ws_profile(a: &Profile, b: &Profile, name: &str) -> Result<Profile, String> {
    let mut result = Profile::new(name);
    result.extends = match (a.extends.clone(), b.extends.clone()) {
        (Some(x), Some(y)) if x != y => {
            return Err(format!(
                "Conflict in 'extends' for profile '{name}': '{x}' vs '{y}'"
            ));
        }
        (x, y) => y.or(x),
    };
    result.abstract_ = a.abstract_ || b.abstract_;
    for key in keys_union(a, b) {
        let merged = match (a.get(key), b.get(key)) {
            (Some(x), Some(y)) => merge_ws_value(key, x, y, name)?,
            (Some(v), None) | (None, Some(v)) => v.clone(),
            (None, None) => continue,
        };
        result.values.insert(key, merged);
    }
    // Carry rejected settings from both workspaces, even when the key ends up
    // with a valid value: the panel shows the effective value and notes the
    // rejected one, so the failure is never hidden.
    for (key, items) in a.rejected.iter().chain(b.rejected.iter()) {
        result
            .rejected
            .entry(*key)
            .or_default()
            .extend(items.iter().cloned());
    }
    Ok(result)
}

fn merge_ws_value(
    key: ConfigKey,
    a: &ConfigValue,
    b: &ConfigValue,
    profile: &str,
) -> Result<ConfigValue, String> {
    // Local keys do not cause workspace conflicts
    let local = registry().get(&key).map(|s| s.local).unwrap_or(false);
    match (a, b) {
        (ConfigValue::Scalar(sa), ConfigValue::Scalar(sb)) => {
            if !local && sa.value() != sb.value() {
                return Err(format!(
                    "Conflict detected in '{profile}' for key '{}': {} vs {}",
                    key.as_str(),
                    sa.value(),
                    sb.value()
                ));
            }
            let sources: HashSet<String> = sa.sources().union(sb.sources()).cloned().collect();
            Ok(ConfigValue::scalar(sa.value().clone(), sources))
        }
        // Lists union across workspaces (never conflict).
        (ConfigValue::List(la), ConfigValue::List(lb)) => {
            let all = la.iter().chain(lb).cloned();
            Ok(ConfigValue::List(group_sourced_iters(all).collect()))
        }
        // Diagnostic settings merge (per-key), filters concatenate.
        (ConfigValue::DiagSettings(sa), ConfigValue::DiagSettings(sb)) => Ok(
            ConfigValue::DiagSettings(merge_sourced_diagnostic_setting_map(sa, sb)),
        ),
        (ConfigValue::DiagFilters(fa), ConfigValue::DiagFilters(fb)) => Ok(
            ConfigValue::DiagFilters(fa.iter().chain(fb).cloned().collect()),
        ),
        _ => Ok(a.clone()),
    }
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn fill_defaults(set: &mut ProfileSet) {
    for profile in set.values_mut() {
        for &key in ConfigKey::all() {
            if !profile.values.contains_key(&key)
                && let Some(default) = key.default_value()
            {
                profile.values.insert(key, default);
            }
        }
    }
}
