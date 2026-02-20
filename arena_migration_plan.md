## 1st phase: CLI mode only

### 1.1. Have a CLI only version (done)
- Remove any dependencies to features in odoo.rs

TO DECIDE: start with a fresh code (copy paste cli parts) of comment out every thing?
- having a diff of the actual changes is useful
    - maybe:
    - copy paste stuff, commit
    - then make incremental changes migration to arena, commiting along the way

### 1.2 Migrate to SymbolTable
[ ] symbol_table module
[ ] adapt Symbol variants definitions
[ ] change symbol creation/lookup methods
    - move to SymbolTable:
        - creation (add_new_variable, etc)
        - navigation (get_ree, get_file...)
        - cross symbol mutations (add_dependency, invalidate, unload...)
        - follow_ref makes little sense to be a symbol associated function (it's not a method anyway)
    - keep as Symbol methods:
        - data acessors: name, typ, parent
    - roughly: local operation on the variants, keep the dispatching on Symbol. Cross symbol boundaries, move to SymbolTable
[ ] follow compiler errors: adapt functions (bottom-up)
[ ] test output: compare with pre_refactor_diagnostics.json

### 1.3 left for later:
[ ] split PackageSymbol slotmap into 2 different ones

- do we need self_key?


## 2nd phase: server + features, 

## For later
RefCell to FileInfo, EntryPoint

## Architectural decisions
- one slot map per Symbol variant
  - optimal memory layout
  - split borrow advantage (can mutably borrow for 2 separate maps)
  - cache locality not as good a single map for tree traversals
- store SymbolTable in SyncOdoo
  - the alterative would be store somewhere else, and pass it around as sibling field to SyncOdoo inside SessionInfo. This might make some borrow issues easier to resolve (when needing to mutate sync odoo while borrowing from the symbol table)

- to decide: evaluations on secondary map?