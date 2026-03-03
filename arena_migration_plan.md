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
- moved ext_symbol/decl_ext_symbol storage from symbol types to symbol table
    - each empty map (the vast majoritiy of them) wastes 24 bytes (so 48 per symbol)
- moved dependency-related methods from variants to a new Depencency trait, to remove code duplication (6 variants)

- to decide: evaluations on secondary map?
- to decide: store entry point in a slotmap, sibling to symbol table under sync_odoo?
    - EntryPointMgr, which owns EntryPoints, is already a child of sync_odoo


## Current changes
-> a typ() method on SymbolKey enum could be handy (even though we could pattern match on the key)
        -> missing package inner type...

-idea: WeakKeysSet type: replicate PtrWeakSet. Offer an iter method that filters out stale keys
    - problem: stale keys accumulate
        - solution would be to mutate it and remove. But needs &mut access.
            - good use of RefCell in this case?



## Strategy
Symbol methods
- start with "leaf" methods first! Clear dependencies before starting to convert a method.

- takes session -> free/associated function. Self becomes symbol key.
    - SymbolView is already a reference to SymbolTable, cannot take mut session (which owns the table)
- borrows (other) symbols -> SymbleTable method
    - self becomes symbol key, from which symbol can be fetched
- immutable methods (&self) -> move to SymbolView
- &mut self methods: move to SymbolTable, self becomes symbol key

Weak.upgrade()?.borrow -> TableSymbol.get_symbol(key)

## Insights/Notes
Instead of Symbols, we now have the separate types stored. And the functions that
return a symbol key could return the specific one. This allows us to:
- look into the specific slot map and get specific type
- cast it to SymbolKey (via into) if needed.

Before, we were stuck with symbols, and using Symbol::as_* when we know the type.
Now we don't need that when we have the specific key.

Code repetition: got rid of code repetition on get_decl_ext_symbol for each symbol variant
                same for get_ext_symbol


Convention: former `self` on Symbol methods -> `target` (SymbolKey)

## Refactor oportunities for later

### NamespaceSymbol::add_file

C-syle loop for finding most specific dir, could be written in more idiomatic Rust:
```rust
  let best = self.directories.iter()
      .enumerate()
      .filter(|(_, dir)| PathBuf::from(path).starts_with(&dir.path))
      .max_by_key(|(_, dir)| dir.path.len());

  match best {
      Some((idx, _)) => self.directories[idx].module_symbols.insert(oyarn!("{}", name), file),
      None => panic!("No valid path found..."),
  };
```

### SymbolTable::get_tree

repeated logic before and inside the loop, handling the root case differently (which is likely wrong). If never called with a Root symbol (or with "Root" in the result is not correct), could be simplified:
```rust
  pub fn get_tree(&self, key: SymbolKey) -> Tree {
      let mut res = (vec![], vec![]);
      let mut current_key = key;
      loop {
          let current = self.get_symbol(current_key).expect("valid key");
          if current.typ() == SymType::ROOT {
              break;
          }
          if current.is_file_content() {
              res.1.insert(0, current.name().clone());
          } else {
              res.0.insert(0, current.name().clone());
          }
          match current.parent() {
              Some(parent) => current_key = parent,
              None => break,
          }
      }
      res
  }
```

### get_main_entry_tree

- a bit inefficient to call get_tree again, instead of keeping a copy of the original one
- the loop for removing the common intial part could be simplied by comparing slices:
```rust
    let len = odoo_tree.len();
    if &tree.0[..len] == odoo_tree {
        tree.0.drain(..len);
    }
```

### i_ext in PythonPackage

It could by a `&'static str`, set at construction (it never changes)
The setter is only used right after its creation.
And the Module variant never gets set to other than ""

### session everywhere
Many methods take session, while all they need is sync_odoo

### &PathBuf x &Path
Consider using &Path (the equivalent of &str) instead of the former

### ext_symbols / decl_ext_symbols - done
Remove them from the symbol types structs, as each empty map (the vast majoritiy of them) wastes 24 bytes (so 48 per symbol). Add them to SymbolTable, under some kind of struct/abstraction (map of maps of maps is quite confusing)

### dead code?
In XmlFileSymbol and CsvSymbol:
```
pub sections: Vec<SectionRange>,
pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
//--- dynamics variables
pub ext_symbols: Hash
```

### compare versions
struct Semver{(u16, u16, u16)};
implement partialeq against (u16, u16, u16) and (u16, u16)
use Semver to store version in SyncOdoo.


## Next big steps

### split package and module
    - PyPackageKey and ModuleKey
    - store ModuleKey's in sync_odoo.modules, update everywhere that takes a PackageKey