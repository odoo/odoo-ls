# Configuration module

Reads, merges, and resolves `odools.toml` files into the runtime
[`ConfigEntry`] used by the language server. The pipeline is **spec-driven and
macro-free**: every setting is declared once as a `FieldSpec` row; parsing,
merging, defaults, schema, restart-detection, and the runtime getter are all
derived from that declaration.

## Module layout

| File | Purpose |
|---|---|
| `config_key_spec.rs` | **The single source of truth.** The `ConfigKey` enum + `specs() -> Vec<FieldSpec>`: one row per setting (kind, serde name, default, merge rule, restart flag, stage hooks). |
| `build.rs` | The pipeline that ties everything together: per-workspace dir-walk + `extends` + version expansion, cross-workspace merge, `fill_defaults`, and the `run_stage_hooks` dispatch for each stage; produces the `ConfigEntry` map + `ConfigView`. |
| `value.rs` | The data model flowing through the pipeline: public value enums (`DiagMissingImportsMode`, `DiagnosticFilter`, …), the `Sourced<T>` provenance wrapper + merge helpers + shared `config_dir`, the internal `Scalar`/`ConfigValue`, and the `Profile` working structure. |
| `spec.rs` | The field-declaration system behind each `specs()` row: `ConfigFieldSpecKind`, `FieldSpec`/`MergeRule`/`Stage`/`StageHook`, and the `registry()`/`all()`/`from_name()` derived from `specs()` (plus `ConfigKey`'s accessors). |
| `parse.rs` | Spec-driven TOML → `Profile` (dispatch on `ConfigFieldSpecKind`); rejects unknown keys. |
| `stages.rs` | Per-field stage hooks (language expansion, path canonicalization, addon/odoo inference, filter-pattern expansion). |
| `version.rs` | `${detectVersion}`/`${splitVersion}` (set-level) + `$version` value resolution. |
| `runtime.rs` | [`ConfigEntry`] (the runtime config: resolved map + typed getters) and `needs_restart`. |
| `render.rs` | `ConfigView` — render view for the IDE config panel. |
| `schema_gen.rs` | `config_json_schema()` — JSON schema emitted from `specs()`. |

## Pipeline

```
TOML ──parse (spec-driven)──▶ Profiles
  per workspace:  dir-walk merge → expand ${version} profiles → extends
                  → ResolveVersionValue → ResolvePaths → AutoDetectPaths (hooks)
  across sources: cross-workspace merge (scalars agree-or-error; lists union)
                  → infer odoo_path → fill defaults → PostMergeProcessing hooks
  ──▶ ConfigEntry map (runtime) + ConfigView (render)
```

Merge rule per field comes from its `FieldSpec` (`ScalarChildWins`,
`ListAlwaysMerge`, `ListControlledBy(key)`, `DiagMap`, `DiagFiltersConcat`).

## Adding a new setting

1. Add a `ConfigKey` variant (`config_key_spec.rs`).
2. Add one `FieldSpec` row in `specs()`:
   ```rust
   FieldSpec::new(MyFlag, "my_flag", Bool).default(ConfigValue::bool(true))
   //   add .triggers_restart(), .merge(rule), and/or .stage(Stage::X, hook) as needed
   ```
   If it needs custom processing, write a hook function in `stages.rs` and
   attach it with `.stage(...)`.
3. (Optional) Implement a getter under ConfigEntry for convenience.

Consumers read it with `config.as_bool(ConfigKey::MyFlag)` (generic getters:
`as_bool`/`as_u64`/`as_str`/`opt_str`/`str_set`). **Parsing, JSON schema,
defaults, restart-detection, and the runtime getter need no per-field edit.**
(The existing fields also have ergonomic named shims like `config.file_cache()`;
new fields don't necessarily require one, but could be a convenvience method to implement)
