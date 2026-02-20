## 1st phase: CLI mode only

### Have a CLI only version (done)
- Remove any dependencies to features in odoo.rs

TO DECIDE: start with a fresh code (copy paste cli parts) of comment out every thing?
- having a diff of the actual changes is useful
    - maybe:
    - copy paste stuff, commit
    - then make incremental changes migration to arena, commiting along the way

### Migrate to SymbolTable
[ ] symbol_table module
[ ] adapt Symbol variants definitions
[ ] follow compiler errors: adapt functions (bottom-up)
[ ] test output: compare with pre_refactor_diagnostics.json


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
