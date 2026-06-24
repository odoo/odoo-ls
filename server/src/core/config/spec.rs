//! The config **field-declaration system**: how a setting is described and how
//! those descriptions are indexed.
//!
//! The single source of truth is the `Vec<FieldSpec>` in `config_key_spec.rs`;
//! this module defines the `FieldSpec` shape (kind, default, merge rule, restart
//! flag, stage hooks) and derives the registry, declaration order, and name
//! lookup from that list. The `ConfigKey` enum is declared in
//! `config_key_spec.rs` next to its rows; its inherent accessors live here.

use std::sync::LazyLock;

use crate::utils::HashMap;

use super::config_key_spec::ConfigKey;
use super::paths::SymlinkPolicy;
use super::value::{ConfigValue, PipelineCtx, Profile};

// ---------------------------------------------------------------------------
// Field kind
// ---------------------------------------------------------------------------

/// The shape a key accepts — used for parsing and JSON-schema emission later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigFieldSpecKind {
    Bool,
    U64,
    Str,
    StrList,
    DiagImportsMode,
    MergeMethod,
    DiagSettings,
    DiagFilters,
}

// ---------------------------------------------------------------------------
// Field behavior (the implementation a FieldSpec references)
// ---------------------------------------------------------------------------

/// Stages that host per-field hooks (dispatched by `build::run_stage_hooks`).
/// The global steps — version-profile expansion, `extends`, cross-workspace
/// merge — are driven directly by `build.rs` and don't appear here. `build.rs`
/// runs these in order: ResolveVersionValue → ResolvePaths → AutoDetectPaths
/// (per source), then PostMergeProcessing (after the cross-workspace merge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Turn a `$version` path into a version string.
    ResolveVersionValue,
    /// Expand `${...}` templates and canonicalize relative paths to absolute.
    ResolvePaths,
    /// Auto-detect paths left unset (mainly `addons_paths`).
    AutoDetectPaths,
    /// Final adjustments after the cross-workspace merge (e.g. language expansion).
    PostMergeProcessing,
}

/// How a field combines when a child profile/file overrides a parent.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum MergeRule {
    /// Scalars: child value wins, else inherit parent.
    #[default]
    ScalarChildWins,
    /// Lists always concatenated (e.g. `additional_languages`).
    ListAlwaysMerge,
    /// Lists whose merge-vs-override is driven by a sibling key (e.g.
    /// `addons_paths` controlled by `addons_merge`). The control key is read
    /// from the *child* (overriding) profile only — the deeper `odools.toml` in
    /// a dir-walk, or the extending profile via `extends` — so the overrider
    /// decides how its list combines with the parent's; a control key set only
    /// in the parent is intentionally ignored.
    ListControlledBy(ConfigKey),
    /// Diagnostic-settings map merge.
    DiagMap,
    /// Diagnostic-filters list concatenation.
    DiagFiltersConcat,
}

/// A per-field transform attached to a pipeline stage. Defined as a normal
/// function elsewhere and attached via `FieldSpec::stage`.
pub(crate) type HookFn = fn(&mut Profile, ConfigKey, &PipelineCtx) -> Result<(), String>;

#[derive(Clone)]
pub(crate) struct StageHook {
    pub stage: Stage,
    pub run: HookFn,
}

/// Everything the pipeline knows about a field, in one idiomatic value.
#[derive(Clone)]
pub(crate) struct FieldSpec {
    pub key: ConfigKey,
    pub name: &'static str,
    pub kind: ConfigFieldSpecKind,
    pub default: Option<ConfigValue>,
    pub merge: MergeRule,
    pub restart: bool,
    /// local configs that do not cause conflict among workspaces
    pub local: bool,
    /// How path-valued fields canonicalize symlinks (see `paths::resolve_path`).
    /// Defaults to `Resolve`; only path settings consult it.
    pub symlink_policy: SymlinkPolicy,
    pub stages: Vec<StageHook>,
}

impl FieldSpec {
    /// A spec with kind-implied defaults; refine it with the builders below.
    pub(crate) fn new(key: ConfigKey, name: &'static str, kind: ConfigFieldSpecKind) -> Self {
        let merge = match kind {
            ConfigFieldSpecKind::StrList => MergeRule::ListAlwaysMerge,
            ConfigFieldSpecKind::DiagSettings => MergeRule::DiagMap,
            ConfigFieldSpecKind::DiagFilters => MergeRule::DiagFiltersConcat,
            _ => MergeRule::default(),
        };
        FieldSpec {
            key,
            name,
            kind,
            default: None,
            merge,
            restart: false,
            local: false,
            symlink_policy: SymlinkPolicy::default(),
            stages: Vec::new(),
        }
    }
    pub(crate) fn default(mut self, value: ConfigValue) -> Self {
        self.default = Some(value);
        self
    }
    pub(crate) fn triggers_restart(mut self) -> Self {
        self.restart = true;
        self
    }
    /// Mark this field as per-source-local (see `FieldSpec::local`).
    pub(crate) fn local(mut self) -> Self {
        self.local = true;
        self
    }
    pub(crate) fn merge(mut self, rule: MergeRule) -> Self {
        self.merge = rule;
        self
    }
    pub(crate) fn symlink_policy(mut self, policy: SymlinkPolicy) -> Self {
        self.symlink_policy = policy;
        self
    }
    pub(crate) fn stage(mut self, stage: Stage, run: HookFn) -> Self {
        self.stages.push(StageHook { stage, run });
        self
    }
}

// ---------------------------------------------------------------------------
// Registry — derived from the spec list
// ---------------------------------------------------------------------------

struct Registry {
    by_key: HashMap<ConfigKey, FieldSpec>,
    /// Declaration order (also the field display order for the config panel).
    order: Vec<ConfigKey>,
    by_name: HashMap<&'static str, ConfigKey>,
}

fn build_registry() -> Registry {
    let mut by_key = HashMap::default();
    let mut order = Vec::new();
    let mut by_name = HashMap::default();
    for spec in super::config_key_spec::specs() {
        let key = spec.key;
        order.push(key);
        if by_name.insert(spec.name, key).is_some() {
            panic!("duplicate config key name: {}", spec.name);
        }
        if by_key.insert(key, spec).is_some() {
            panic!("duplicate config key: {key:?}");
        }
    }
    Registry {
        by_key,
        order,
        by_name,
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(build_registry);

/// The field registry, keyed by `ConfigKey`.
pub(crate) fn registry() -> &'static HashMap<ConfigKey, FieldSpec> {
    &REGISTRY.by_key
}

impl ConfigKey {
    /// All keys in declaration order.
    pub(crate) fn all() -> &'static [ConfigKey] {
        &REGISTRY.order
    }
    /// Reverse lookup from a serialized name.
    pub(crate) fn from_name(name: &str) -> Option<ConfigKey> {
        REGISTRY.by_name.get(name).copied()
    }
    fn spec(self) -> &'static FieldSpec {
        REGISTRY
            .by_key
            .get(&self)
            .expect("every ConfigKey has a spec in config_key_spec::specs()")
    }
    pub(crate) fn as_str(self) -> &'static str {
        self.spec().name
    }
    pub(crate) fn kind(self) -> ConfigFieldSpecKind {
        self.spec().kind
    }
    pub(super) fn symlink_policy(self) -> SymlinkPolicy {
        self.spec().symlink_policy
    }
    pub(super) fn default_value(self) -> Option<ConfigValue> {
        self.spec().default.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `build_registry` panics at `LazyLock` init on a duplicate key or name.
    /// Touch the registry here so that programmer error surfaces in CI rather
    /// than only at first runtime use. Also pins that every `ConfigKey` has a
    /// spec (so the accessors below cannot panic).
    #[test]
    fn registry_builds_and_covers_every_key() {
        let reg = registry();
        for &key in ConfigKey::all() {
            assert!(reg.contains_key(&key), "missing spec for {key:?}");
            // Exercises `spec()`, which panics if the key is unregistered.
            let _ = key.kind();
            assert_eq!(ConfigKey::from_name(key.as_str()), Some(key));
        }
        assert_eq!(reg.len(), ConfigKey::all().len());
    }
}
