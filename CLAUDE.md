# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

OdooLS: a language server (Rust) providing autocompletion, diagnostics, hover, go-to-definition,
find-references, semantic tokens, etc. for Odoo development — across Python, XML (views, OWL
templates), JS (OWL components), and CSV (data files). All source lives under `server/`; the
VS Code extension that bundles this server is a separate repo (`odoo-vscode`).

## Common commands

All commands run from `server/` (the crate root — there is no top-level Cargo.toml).

```bash
cd server
cargo build                              # debug build
cargo build --release
cargo run --bin odoo_ls_server -- --use-tcp   # run the server over TCP for debugging (see .vscode/launch.json)
cargo clippy --no-default-features --all-targets -- -D warnings   # must be warning-free; enforced by the pre-commit hook
cargo test                                # unit + most integration tests
cargo test --test test_basics             # run one integration test file
cargo test some_test_name -- --nocapture  # run a single test, with output
```

**Most integration tests require `COMMUNITY_PATH`** to point at a real Odoo Community checkout
(they build a real symbol table against it). Without it, those tests panic rather than skip:

```bash
COMMUNITY_PATH=/path/to/odoo cargo test
```

`test_js_owl_features.rs` additionally gates on a `TSSERVER` env var (path/command for a
`tsserver` binary); without it, that suite *skips* rather than fails, so plain `cargo test`
stays green on a machine without TypeScript installed.

Activate the repo's pre-commit hook once per clone (it runs the clippy check above on staged
`.rs`/`Cargo.*`/`clippy.toml` changes):

```bash
git config core.hooksPath .githooks
```

Cross-compiled release builds (Windows/Linux/macOS x64+arm64) go through `server/build.sh`
(requires Docker; see `build.sh init`) — not needed for normal development.

`typeshed` (`server/typeshed`) is a git submodule (Python stdlib/third-party stubs); run
`git submodule update --init` if it's empty. `server/additional_stubs` holds extra bundled
stub packages (e.g. `lxml`) not covered by typeshed.

## Architecture

### Build pipeline (the core mental model)

Every symbol (module, class, function, file, XML template, …) advances through ordered build
steps, tracked per-symbol as pending/in-progress/done:

```
ARCH ──▶ ARCH_EVAL ──▶ ODOO_FUNCTION_AE ──▶ VALIDATION
```

- **ARCH** (`python_arch_builder.rs`, `xml_arch_builder.rs`, `js_arch_builder.rs`,
  `csv_arch_builder.rs`): parse and build the raw symbol tree (classes, functions, imports,
  templates) without resolving types.
- **ARCH_EVAL** (`python_arch_eval.rs`): resolve types/evaluations for module and class-level
  code.
- **ODOO_FUNCTION_AE** (`python_function_arch.rs`): a split-out phase specifically for function
  bodies' arch+eval, run separately from ARCH_EVAL — see `EAGER_METHOD_ARCH_BUILD` in
  `constants.rs` for why (avoids a measured 66% duplicate-build rate on class methods).
- **VALIDATION** (`python_validator.rs`, `xml_validation.rs`, `csv_validation.rs`,
  `js_validator.rs`): produce diagnostics once a symbol and its dependencies are fully built.

`build_scheduler.rs` (`BuildScheduler`) holds one FIFO queue per step and drains them in order
(`build_one`); `odoo.rs` (`SyncOdoo`) owns the scheduler, the global `SymbolTable`, and
orchestrates full/incremental rebuilds (`build_modules`, module load ordering via
`module_load_order.rs`). Rebuilding is incremental and dependency-driven: changing a symbol
invalidates and re-queues dependents (`symbols/storage/dependency_mgr.rs`,
`symbols/storage/lifecycle.rs`) rather than rebuilding everything.

`SessionInfo` (`threads.rs`) is the per-request handle threaded through nearly every function;
it wraps `&mut SyncOdoo` plus the LSP send/receive channels. `EntryPoint`/`EntryPointMgr`
(`entry_point.rs`) represent the distinct roots being tracked (main Odoo install, each addon
path, standalone/untitled files).

### Symbol table

`core/symbols/` defines the symbol kinds (`ClassSymbol`, `FunctionSymbol`, `ModuleSymbol`,
`FileSymbol`, `NamespaceSymbol`, XML/CSV/JS file symbols, …); `core/symbols/storage/` holds
their slotmap-backed storage, weak-key references (`symbol_keys.rs`), parent/child linkage
(`parents.rs`), and the dependency graph. `symbol_table_impl.rs` is the table itself.

### Config system

`core/config/` is spec-driven and macro-free: every `odools.toml` setting is declared once as
a `FieldSpec` row (`config_key_spec.rs`), and parsing, merging, defaults, JSON schema,
restart-detection, and the runtime getter are all derived from that one declaration — see
`core/config/README.md` for the full pipeline and "adding a new setting" steps. Read it before
touching config code.

### JS / OWL support

JS features (hover, go-to-def, completion, references, semantic tokens) for expressions
embedded in OWL XML templates (`t-if`, `t-out`, `t-on-*`, props, …) work by building a virtual
`.js` document per component that reconstructs the template's evaluation context, forwarding
requests to a real `tsserver` process (`core/tsserver_bridge.rs`), and mapping results back
onto the XML. See `server/docs/owl-virtual-docs.md` for the full mechanism; main code is
`features/owl_virtual.rs`, `features/owl_expr.rs`, `core/js_import_graph.rs`,
`core/js_arch_builder.rs`.

### Debugging tools

`server/debug_tools/`: a GDB pretty-printer script for the symbol table
(`symbol_table_gdb_script.py`) and for weak references (`rust_ref_printers.py`) — wired up via
`.vscode` debug config, see `debug_tools/README.md`. `core/perf_probe.rs` is a zero-cost
(when disabled) debug-only timing probe for the ODOO_FUNCTION_AE per-method build path (see
`EAGER_METHOD_ARCH_BUILD` in `constants.rs`).
