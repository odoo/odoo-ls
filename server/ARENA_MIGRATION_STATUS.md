# Arena Migration Status Report

**Date:** 2026-03-06
**Branch:** `slotmap_experiment`
**Goal:** Replace `Rc<RefCell<Symbol>>` with slotmap-based arena (`SymbolTable`). Phase 1: CLI mode only.

## Architectural Changes (Beyond Slotmaps)

The migration was an opportunity to clean up structural issues in the codebase:

### Centralized ExtSymbolStore

Previously, `ext_symbols` and `decl_ext_symbols` were duplicated as fields across every variant struct that could host them (FileSymbol, ClassSymbol, FunctionSymbol, PackageSymbol, NamespaceSymbol). Each variant had its own copy of `get_ext_symbol` and `get_decl_ext_symbol` methods with identical logic.

Now: a single `ExtSymbolStore` on `SymbolTable` holds both maps (`owners_by_target`, `symbols_by_owner`), with one `add`, one `get`, and one `remove` method. The dead fields on variant structs can be removed.

### Trait-Based Code Deduplication

**`Dependencies` trait + `impl_dependencies!` macro** (`dependency_mgr.rs`): The 6 buildable variants (File, Class, Function, PythonPackage, Module, XmlFile) all had identical dependency/dependent field management code. Now a single trait defines the interface, and a macro generates the impl for each variant. `add_dependency` lives on `SymbolTable` and handles both sides (dependent + dependency) in one call.

**`SymbolMgr` trait**: Section management (`add_section`, `get_section_for`, `change_parent`) and symbol storage (`get_sections`, `get_symbols`) stay on the trait. Lookup methods that needed cross-symbol access (`get_content_symbol`, `_get_loc_symbol`, `get_all_visible_symbols`) moved to `SymbolTable`, taking `&dyn SymbolMgr` where needed to avoid re-matching variants.

### Methods Moved to Free Functions / SymbolTable Methods

Many methods that were on the `Symbol` enum (dispatching to variants) became either:
- **Free functions** in `symbol_table.rs` / `symbol_table_create.rs` taking a `SymbolKey` parameter (for complex logic needing `&mut session`: `invalidate`, `unload`, `follow_ref`, `next_refs`, `get_member_symbol`, etc.)
- **Methods on `SymbolTable`** (for logic only needing `&self` or `&mut self`: `get_ext_symbol`, `is_class_descriptor`, `is_func_overloaded`, etc.)

### WeakSet / WeakKey Abstractions

`PtrWeakHashSet<Weak<RefCell<Symbol>>>` was replaced by `WeakSet<K>` — a generic set that doesn't know about `Weak`/`Rc` at all. Staleness is determined by a caller-provided closure (`iter_valid(|&k| st.contains_key(k))`), making the concept reusable for any key type.

Similarly, `WeakKey<K>` wraps any key with `upgrade(&impl ContainsKey<K>)` semantics, replacing the old `Weak<RefCell<Symbol>>` field pattern.

### `get_sym!` Macro

`get_sym!(st, key)` is a convenience macro that expands to `st.get_symbol_view(key).expect("valid key")`. It replaces the old `.borrow()` on `Rc<RefCell<Symbol>>`. When the needed method is available on `SymbolView`, you can chain it: `get_sym!(st, key).name()`. For methods that have been moved to `SymbolTable`, the call becomes `st.something(key)` instead.

Handy during conversion as a mechanical substitution, but due to be phased out as more direct accessors are added to `SymbolTable` (see below).

### Phasing Out SymbolView

`SymbolView` is an enum of references (`&FileSymbol`, `&ClassSymbol`, etc.) returned by `get_symbol_view(key)`. The problem: since `get_symbol_view` returns a temporary, you can't chain method calls that return references — `st.get_symbol_view(key).evaluations()` won't compile because the `SymbolView` temporary is dropped before the returned reference can be used. You must bind the view to a variable first, which is verbose.

The plan is to add **direct accessor methods on `SymbolTable`** that take a `SymbolKey` and dispatch internally: `st.name(key)`, `st.parent(key)`, `st.evaluations(key)`, etc. These go straight to the slotmap field, bypassing `SymbolView` entirely. Some already exist (e.g. `get_evaluations`). Over time, most call sites should use direct accessors, with `SymbolView` reserved only for rare cases needing full variant dispatch.

Note that branching on the variant type can always be done by matching on `SymbolKey` itself (it's an enum: `SymbolKey::File(_)`, `SymbolKey::Class(_)`, etc.) — no need for `SymbolView` just to check the variant. This is also why splitting `PackageSymbol` into separate `ModuleKey`/`PythonPackageKey` slotmaps is planned: it makes the key enum fully granular, eliminating the last case where you'd need to look inside the data to distinguish variants.

### FifoWeakHashSet Generalized

`FifoWeakHashSet<RefCell<Symbol>>` (for rebuild queues) became `FifoWeakHashSet<SymbolKey>`, using the same closure-based validity check pattern as `WeakSet`.

---

## What's Been Done

### Core Infrastructure (Complete)

- **SymbolTable** (`symbol_table.rs`): Central arena with per-variant slotmaps, `SymbolView` enum, core accessors, tree navigation (`get_tree`, `get_in_parents`, `get_root`, `get_file`, `find_module`), symbol lookup (`get_symbol`, `get_sub_symbol`, `get_content_symbol`, `_get_loc_symbol`, `get_all_visible_symbols`), member resolution (`get_member_symbol`, `all_members`), ref following (`follow_ref`, `next_refs`), scope/parent queries, `invalidate`, `previous_step_done`
- **Symbol Creation** (`symbol_table_create.rs`): All `add_new_*` methods returning typed keys, `create_from_path`, `unload`, `remove_symbol`, `remove`
- **ExtSymbolStore** (`ext_symbol_store.rs`): Centralized ext_symbols/decl_ext_symbols, `owners_by_target` uses `WeakSet<SymbolKey>`, explicit `remove()` for cleanup on unload
- **Dependencies** (`dependency_mgr.rs`): `Dependencies` trait + macro for 6 buildable variants, `WeakSet<SymbolKey>` for dep/dependent sets, `add_dependency` on SymbolTable
- **WeakSet/WeakKey** (`weak_hash_set.rs`, `symbol_table.rs`): Generic `WeakSet<K>` with `iter_valid`, `WeakKey<K>` wrapper, `ContainsKey<K>` trait

### Symbol Variants (Complete)

- **ModuleSymbol**: Fully converted — `is_in_deps`, `load_module_info`, `_load_depends`, `_load_arch`, `check_data`, `validate_manifest`, `get_xml_id`, `load_data`
- **ClassSymbol**: `bases` → `Vec<ClassKey>`, `inherits`, `is_class_descriptor`, `is_field_class` (with cache), `is_field`, `is_method`, `is_inheriting_from_field`
- **FunctionSymbol**: `is_overloaded`, `get_indexed_arg_in_call`, `add_return_evaluations`
- **VariableSymbol**: `get_relational_model` → free function
- **Model**: Fully converted — `symbols` → `HashSet<ClassKey>`, `dependents` → `WeakSet<SymbolKey>`

### SyncOdoo (`odoo.rs`) (Partially Complete)

- `rebuild_arch/arch_eval/validation` → `FifoWeakHashSet<SymbolKey>`
- `modules` → `HashMap<OYarn, PackageKey>`
- `build_now`, `build_now_dependencies`, `pop_item` — converted
- `add_to_rebuild_*`, `remove_from_rebuild*`, `is_in_rebuild` — converted
- `get_symbol` — converted

### Evaluation (Partially Complete)

- `from_sections`, `eval_from_symbol` converted
- `EvaluationSymbol.get_symbol` param → `Option<SymbolKey>`
- `get_eval_out_of_function_scope` takes `FunctionKey`
- `ContextValue::MODULE/SYMBOL` → `WeakKey<SymbolKey>`

### Other

- **EntryPoint**: `not_found_symbols` and `not_found_symbols_for_models` → `WeakSet<SymbolKey>`
- **`pub mod symbol` commented out** in `mod.rs` — the old `Symbol` enum is disabled

### Recently Done (This Session)

- `ExtSymbolStore.owners_by_target` inner set changed from `HashSet<SymbolKey>` to `WeakSet<SymbolKey>` (matches old `PtrWeakHashSet` behavior)
- `ExtSymbolStore::remove(key)` added — cleans up both `owners_by_target` and `symbols_by_owner` on unload
- Renamed fields for clarity: `owners` → `owners_by_target`, `declarations` → `symbols_by_owner`
- Renamed type aliases: `ExtSymbolOwners` → `OwnersBySymbolName`, `DeclExtSymbols` → `DeclsByTarget`

---

## What's Left (CLI Mode)

### Remaining `PtrWeakHashSet` → `WeakSet` Conversions

These fields still use the old `PtrWeakHashSet<Weak<RefCell<...>>>` type and need conversion:

| File | Field | New Type |
|------|-------|----------|
| `file_symbol.rs` | `model_dependencies` | `WeakSet<ModelKey>` or keep `PtrWeakHashSet` if Model stays as `Rc<RefCell<Model>>` |
| `xml_file_symbol.rs` | `model_dependencies` | same |
| `csv_file_symbol.rs` | `model_dependencies` | same |
| `function_symbol.rs` | `model_dependencies` | same |
| `module_symbol.rs` | `model_dependencies` | same |
| `package_symbol.rs` (PythonPackage) | `model_dependencies` | same |

> **Note:** `model_dependencies` depends on whether `Model` gets arenaized. If `Model` stays as `Rc<RefCell<Model>>`, these stay as-is. If Model moves to arena, convert to `WeakSet<ModelKey>`.

### Remaining `Rc<RefCell<Symbol>>` in Variant Structs

Several variant structs still have dead `ext_symbols`/`decl_ext_symbols` fields (now handled by `ExtSymbolStore`). These should be removed:

- `function_symbol.rs`: `ext_symbols`, `decl_ext_symbols`
- `namespace_symbol.rs`: `ext_symbols`
- `package_symbol.rs` (PythonPackage): `ext_symbols`, `decl_ext_symbols`

### Files to Convert (Ordered by Dependency)

#### Phase 1: Import Resolution

**`import_resolver.rs`** (~14 occurrences of `Rc<RefCell<Symbol>>`)
- `ImportResult.symbols` → `Vec<SymbolKey>`
- All resolve functions (`resolve_import_stmt`, `resolve_from_stmt`, `manual_import`) need parameter conversion
- **Blocks:** arch builder, arch eval, hooks

#### Phase 2: Build Pipeline Core

These three files have struct fields partially converted (e.g. `file: SymbolKey`, `sym_stack: Vec<SymbolKey>`) but method bodies still use `.borrow()` patterns on the old types.

1. **`python_arch_builder.rs`** (~1 occurrence left in struct, method bodies need conversion)
   - `entry_point: Rc<RefCell<EntryPoint>>` — stays (EntryPoint not arenaized)
   - `file_info: Option<Rc<RefCell<FileInfo>>>` — stays (FileInfo not arenaized)
   - Method bodies: replace `symbol.borrow()` → `st.get_symbol_view(symbol)` etc.
   - **Depends on:** import_resolver

2. **`python_arch_eval.rs`** (~8 occurrences)
   - Same pattern as arch builder
   - **Depends on:** import_resolver

3. **`python_validator.rs`** (~3 occurrences)
   - `current_module: Option<Rc<RefCell<Symbol>>>` → `Option<PackageKey>` or `Option<SymbolKey>`
   - **Depends on:** import_resolver

#### Phase 3: Hooks

4. **`python_arch_builder_hooks.rs`** (~7 occurrences)
   - Hook function signatures: `Rc<RefCell<Symbol>>` → typed keys (`ClassKey`, `SymbolKey`)
   - **Depends on:** arch builder struct conversion

5. **`python_arch_eval_hooks.rs`** (~79 occurrences — largest single file)
   - ~30+ hook closures, each using `Rc<RefCell<Symbol>>` parameters
   - Most mechanical but high volume
   - **Depends on:** arch eval struct conversion

#### Phase 4: Odoo-Specific Builder

6. **`python_odoo_builder.rs`** (~3 occurrences + 2 `PtrWeakHashSet`)
   - `symbol` field → `ClassKey`
   - `xml_id_locations` insertions: `PtrWeakHashSet::new()` → appropriate `WeakSet`
   - **Depends on:** module_symbol (done), model (done)

#### Phase 5: XML/CSV Pipeline

7. **`xml_arch_builder.rs`** (~2 `PtrWeakHashSet` occurrences)
   - `xml_id_locations` weak set type fix
   - Small file, mostly done

8. **`csv_arch_builder.rs`** (~2 `PtrWeakHashSet` occurrences)
   - Same `xml_id_locations` pattern

9. **`xml_validation.rs`** (~8 occurrences)
   - `xml_symbol` field → `XmlFileKey` or `SymbolKey`
   - Method bodies need conversion

10. **`xml_data.rs`** (~7 occurrences)
    - `OdooData.file_symbol: Weak<RefCell<Symbol>>` → `WeakKey<SymbolKey>` or `WeakKey<XmlFileKey>`

#### Phase 6: Remaining Core

11. **`odoo.rs`** (~6 remaining occurrences)
    - Mostly in code paths that call into unconverted builders

12. **`entry_point.rs`** (~11 occurrences)
    - Some remaining `Rc<RefCell<Symbol>>` in methods that interact with unconverted code

13. **`evaluation.rs`** (~1 remaining occurrence)
    - Minor cleanup

#### NOT needed for CLI (features layer)

These files are LSP-only and can be deferred:

| File | Occurrences |
|------|-------------|
| `features_utils.rs` | 30 |
| `completion.rs` | 55 |
| `xml_ast_utils.rs` | 13 |
| `definition.rs` | 9 |
| `ast_utils.rs` | 4 |
| `references.rs` | 3 |
| `hover.rs` | 3 |
| `workspace_symbols.rs` | 1 |

---

## Planned Structural Changes

### 1. Remove `Option` from `parent` field

Currently all symbol variants have `parent: Option<SymbolKey>`. Plan is to make it non-optional (except `RootSymbol` which has no parent — remove the field there entirely).

**When to do it:** Can be done at any time — it's an isolated refactor within the symbol structs. Doing it early simplifies code (removes `.unwrap()` calls on parent) but isn't blocking anything. Recommend doing it **before Phase 2** so the builder conversions benefit from cleaner parent access.

### 2. Split `PackageSymbol` into separate slotmaps

Currently `PackageSymbol` is an enum (`Module | PythonPackage`) stored in a single `SlotMap<PackageKey, PackageSymbol>`. Plan is to give each its own slotmap with `ModuleKey` and `PythonPackageKey`.

**When to do it:** This is a larger refactor that touches every `SymbolKey::Package(p)` match arm and every place that calls `as_module_package()` / `as_python_package()`. Recommend doing it **after Phase 5** when all the builder conversions are done — otherwise you'd be converting the same code twice. The typed keys can be adapted in the end with search-and-replace on `PackageKey` → `ModuleKey`/`PythonPackageKey`.

**Exception:** If you find that the split would significantly simplify a phase (e.g., module_symbol methods that currently need `match` on `PackageSymbol`), consider doing it earlier for that specific benefit.

---

## Dependency Graph

```
                 import_resolver (Phase 1)
                /         |         \
    arch_builder    arch_eval    validator   (Phase 2)
         |              |
  arch_builder_hooks  arch_eval_hooks       (Phase 3)
                        |
              python_odoo_builder           (Phase 4)
              /         |
   xml_arch_builder  csv_arch_builder       (Phase 5)
         |
   xml_validation + xml_data                (Phase 5)
         |
   odoo.rs / entry_point.rs cleanup        (Phase 6)
```

---

## Key Patterns Reference

### Basic Conversions

| Old Pattern | New Pattern |
|------------|-------------|
| `symbol.borrow().name()` | `st.get_symbol_view(key).name()` or `get_sym!(st, key).name()` |
| `symbol.borrow_mut().set_x(v)` | Direct field access: `st.classes[k].x = v` |
| `self` on Symbol method | `target: SymbolKey` parameter |

### Weak References

| Old Pattern | New Pattern |
|------------|-------------|
| `Rc::downgrade(&symbol)` for storage in a `Weak` field | `key.into()` — produces a `WeakKey<SymbolKey>` |
| `Rc::downgrade(&symbol)` where key is just passed around | Just use `SymbolKey` (it's Copy, no need for weak) |
| `weak.upgrade()` → `Option<Rc<..>>` | `weak_key.upgrade(&st)` → `Option<SymbolKey>` |
| `weak.upgrade().is_some()` | `st.contains_key(key)` |
| Field type `Weak<RefCell<Symbol>>` | `WeakKey<SymbolKey>` |

### Weak Collections

| Old Pattern | New Pattern |
|------------|-------------|
| `PtrWeakHashSet::new()` + `.insert(Rc::downgrade(&s))` | `WeakSet::new()` + `.insert(key)` |
| `weak_set.iter()` (auto-expire on iterate) | `weak_set.iter_valid(\|&k\| st.contains_key(k))` |
| `HashMap<Rc<RefCell<Symbol>>, V>` | `HashMap<SymbolKey, V>` |
| `PtrWeakKeyHashMap<Weak<..>, V>` | `HashMap<SymbolKey, V>` + explicit `remove()` on unload |

### Breaking Borrow Conflicts

The arena centralizes all symbols in `session.sync_odoo.symbol_table`. Reading a symbol and mutating session (or another symbol) in the same scope causes borrow conflicts. These patterns resolve them:

**Clone-before-loop (evaluations):**
When iterating over a symbol's evaluations while also needing `&mut session` in the loop body, clone the evaluations vec upfront:
```rust
let evals = st!().get_evaluations(key).cloned().unwrap_or_default();
for eval in &evals {
    // can now use &mut session freely here
}
```
This applies to any field read from the symbol table that you need to iterate over while mutating: evaluations, bases, sections, symbols, etc.

**`iter_valid` returns an owned iterator:**
`WeakSet::iter_valid` internally collects into a `Vec` and returns `IntoIter<K>`, so the borrow on the symbol table is released before you start iterating. This means you can use `&mut session` directly in the loop body without an extra collect step:
```rust
for key in weak_set.iter_valid(|&k| st!().contains_key(k)) {
    // st!() borrow is already released — &mut session is fine here
    do_something(session, key);
}
```

**Build queue pattern:**
When the keys to process come from deeply nested or complex iteration (e.g. walking dependents across multiple build steps/levels), collecting with functional style becomes unreadable. Instead, build a queue imperatively, then drain it:
```rust
let mut queue = Vec::new();
for level in st!().dependents(key).iter() {
    for step in level.iter() {
        if let Some(set) = step {
            for &k in set.iter_valid(|&k| st!().contains_key(k)) {
                queue.push(k);
            }
        }
    }
}
for key in queue {
    do_something(session, key); // &mut session is fine here
}
```
The queue separates the read phase (traversing symbol table) from the mutate phase (processing with `&mut session`).

**Read-then-mutate (scalar fields):**
When you need to read a field from one symbol and write to another, copy the value out first:
```rust
let name = st!().get_symbol_view(key).name().clone();
st!().classes[other_key].some_field = name;
```

**`st!()` macro re-expansion:**
The `macro_rules! st { () => { session.sync_odoo.symbol_table } }` pattern avoids lingering borrows — each `st!()` is an independent borrow of `session`. After calling a function that takes `&mut session`, you can use `st!()` again freely (unlike a `let st = &session.sync_odoo.symbol_table` binding which would conflict).

---

## Conversion Cheat Sheet

Step-by-step for mechanically converting a `symbol.borrow().foo()` or `symbol.borrow_mut().foo()` call:

### 1. Identify where `foo` lives now

The old `Symbol` enum dispatched everything through one type. Now methods are spread across three locations:

| Location | How to call | Examples |
|----------|------------|---------|
| **`SymbolView`** method | `get_sym!(st, key).foo()` | `name()`, `typ()`, `parent()`, `range()`, `paths()`, `is_external()`, `as_symbol_mgr()`, `as_class_sym()` |
| **`SymbolTable`** method | `st.foo(key, ...)` | `get_file()`, `get_ext_symbol()`, `is_class_descriptor()`, `set_build_status()`, `get_content_symbol()`, `contains_key()` |
| **Free function** in `symbol_table.rs` / `symbol_table_create.rs` | `foo(session, key, ...)` or `foo(&st, key, ...)` | `invalidate()`, `unload()`, `follow_ref()`, `next_refs()`, `get_member_symbol()`, `all_members()`, `is_field()`, `is_method()`, `get_scope_symbol()` |

Use the IDE's go-to-definition / symbol search to find where a method landed.

### 2. Convert the call

**Reading (was `symbol.borrow().foo()`):**
```rust
// Old:
let x = symbol.borrow().name().clone();

// New — if foo is on SymbolView:
let x = get_sym!(st!(), key).name().clone();

// New — if foo is on SymbolTable:
let file = st!().get_file(key);
st!().set_build_status(key, step, status);

// New — if foo is a free function:
let x = follow_ref(session, key);
```

**Mutating (was `symbol.borrow_mut().field = x`):**
Direct field access through the slotmap, when the variant is known:
```rust
// Old:
symbol.borrow_mut().as_class_sym_mut().bases.push(base);

// New:
st!().classes[class_key].bases.push(base);

// Reading a field directly:
let is_cm = st!().functions[f].is_class_method;
let evals = &st!().functions[f].evaluations;
```

### 3. Convert the variable itself

| Old | New |
|-----|-----|
| `let symbol: Rc<RefCell<Symbol>>` | `let key: SymbolKey` (or typed: `ClassKey`, `FileKey`, etc.) |
| `let symbol: &Rc<RefCell<Symbol>>` | `let key: SymbolKey` (it's Copy, no need for references) |
| `let weak: std::rc::Weak<RefCell<Symbol>>` | `let weak: Weak<SymbolKey>` (the arena `Weak` from `symbol_table`) |
| `symbol.clone()` | just `key` (it's Copy) |
| `Rc::ptr_eq(&a, &b)` | `a == b` (SymbolKey is Eq) |
| `Rc::downgrade(&symbol)` stored in a `Weak` field | `key.into()` — produces a `Weak<SymbolKey>` |
| `Rc::downgrade(&symbol)` just passed around | just `key` (it's Copy, no need for weak) |
| `symbol.borrow().weak_self` | not needed — the key IS the identity |

---

## Files to Ignore

Files prefixed with `_` (e.g., `_odoo.rs`) are snapshots of the original code before conversion. They are not compiled and should be ignored.
