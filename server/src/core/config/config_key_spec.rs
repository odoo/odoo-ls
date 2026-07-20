//! The declarative config table: one `FieldSpec` instance per setting. This is
//! the single source of truth — `registry()`, declaration order, and name
//! lookup are all derived from this list (see `spec.rs`).
//!
//! To add a setting: add a `ConfigKey` variant above, then add one `FieldSpec`
//! row here. If it needs custom processing, write a hook function (in the stages
//! module) and attach it with `.stage(Stage::X, my_hook)`.

use super::paths::SymlinkPolicy;
use super::spec::FieldSpec;
use super::value::ConfigValue;

/// All config keys. Hand-written — just identifiers, no logic. Everything about
/// a key (name, kind, default, behavior) lives in its `FieldSpec` row below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    Version,
    Base,
    OdooPath,
    AddonsPaths,
    AddonsMerge,
    PythonPath,
    Stdlib,
    AdditionalStubs,
    AdditionalStubsMerge,
    AdditionalLanguages,
    FileCache,
    DiagMissingImports,
    AcFilterModelNames,
    AutoRefreshDelay,
    DiagnosticSettings,
    DiagnosticFilters,
    NoTypeshedStubs,
    TsServerCommand,
    DisableJavascript,
    TsCheck,
}

pub(super) fn specs() -> Vec<FieldSpec> {
    use super::spec::ConfigFieldSpecKind::*;
    use super::spec::MergeRule::*;
    use super::spec::Stage::*;
    use super::stages;
    use ConfigKey::*;

    vec![
        // `$version`/`$base` only feed `${version}`/`${base}` path templates.
        // The paths they expand into are themselves restart-flagged, but a
        // version change can also affect a *non*-restart field (e.g. a
        // `diagnostic_filters` path), so flag them restart-triggering directly
        // to guarantee a version/base change always restarts the server.
        FieldSpec::new(Version, "$version", Str)
            .triggers_restart()
            .local()
            .stage(ResolveVersionValue, super::version::resolve_version_value),
        FieldSpec::new(Base, "$base", Str).triggers_restart().local(),
        FieldSpec::new(OdooPath, "odoo_path", Str)
            .triggers_restart()
            .stage(ResolvePaths, stages::canon_odoo_path)
            .stage(PostMergeProcessing, stages::infer_odoo_path),
        FieldSpec::new(AddonsPaths, "addons_paths", StrList)
            .triggers_restart()
            .merge(ListControlledBy(AddonsMerge))
            .stage(ResolvePaths, stages::canon_addons_paths)
            .stage(AutoDetectPaths, stages::infer_addons),
        FieldSpec::new(AddonsMerge, "addons_merge", MergeMethod).local(),
        // Preserve a symlinked leaf: a venv's `bin/python` is a symlink to the
        // base interpreter, and resolving it would hide the venv from Python's
        // `pyvenv.cfg` detection.
        FieldSpec::new(PythonPath, "python_path", Str)
            .triggers_restart()
            .symlink_policy(SymlinkPolicy::PreserveLeaf)
            .stage(ResolvePaths, stages::canon_python_path)
            .stage(PostMergeProcessing, stages::default_python_path),
        FieldSpec::new(Stdlib, "stdlib", Str)
            .default(ConfigValue::str(""))
            .triggers_restart()
            .stage(ResolvePaths, stages::canon_stdlib),
        FieldSpec::new(AdditionalStubs, "additional_stubs", StrList)
            .triggers_restart()
            .merge(ListControlledBy(AdditionalStubsMerge))
            .stage(ResolvePaths, stages::canon_stubs),
        FieldSpec::new(AdditionalStubsMerge, "additional_stubs_merge", MergeMethod).local(),
        FieldSpec::new(AdditionalLanguages, "additional_languages", StrList)
            .stage(PostMergeProcessing, stages::expand_languages),
        FieldSpec::new(FileCache, "file_cache", Bool).default(ConfigValue::bool(true)),
        FieldSpec::new(DiagMissingImports, "diag_missing_imports", DiagImportsMode)
            .default(ConfigValue::str("all")),
        FieldSpec::new(AcFilterModelNames, "ac_filter_model_names", Bool)
            .default(ConfigValue::bool(true)),
        FieldSpec::new(AutoRefreshDelay, "auto_refresh_delay", U64).default(ConfigValue::u64(1000)),
        FieldSpec::new(DiagnosticSettings, "diagnostic_settings", DiagSettings),
        FieldSpec::new(DiagnosticFilters, "diagnostic_filters", DiagFilters)
            .stage(ResolvePaths, stages::expand_filter_patterns),
        FieldSpec::new(NoTypeshedStubs, "no_typeshed_stubs", Bool)
            .default(ConfigValue::bool(false))
            .triggers_restart(),
        FieldSpec::new(TsServerCommand, "tsserver_command", Str) //it could maybe be nice to resolve config variables like {workspaceFolder}
            .triggers_restart()
            .default(ConfigValue::str("tsserver")),
        // Turns off all frontend-asset processing: no tsserver process, no JS/TS
        // files or Owl XML templates from manifest asset bundles are parsed or
        // added to the symbol table, no JS diagnostics/features.
        FieldSpec::new(DisableJavascript, "disable_javascript", Bool)
            .default(ConfigValue::bool(false))
            .triggers_restart(),
        FieldSpec::new(TsCheck, "ts_check", Bool)
            .triggers_restart()
            .default(ConfigValue::bool(false))
    ]
}
