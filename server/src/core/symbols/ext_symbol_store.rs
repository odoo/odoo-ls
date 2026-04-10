use std::collections::HashMap;
use crate::{constants::OYarn, core::symbols::symbol_table::{SymbolTable}, weak_collections::WeakSet};
use crate::core::symbols::symbol_keys::{ContainsKey, SymbolKey};

/// section index → [variable keys]
type SectionSymbols = HashMap<u32, Vec<SymbolKey>>;
/// name → section symbols
type NamedSectionSymbols = HashMap<OYarn, SectionSymbols>;
/// target/host → named section symbols
type DeclsByTarget = HashMap<SymbolKey, NamedSectionSymbols>;
/// name → set of owners
type OwnersBySymbolName = HashMap<OYarn, WeakSet<SymbolKey>>;

/// @arena
/// OwnersBySymbolName: formerly ext_symbols (on each Symbol variant)
/// DeclsByTarget: formerly decl_ext_symbols (on each Symbol variant)
///   - `owners_by_target[target][name] → WeakSet<owner keys>`
///   - `symbols_by_owner[owner][target][name][section] → Vec<variable keys>`
#[derive(Debug)]
pub struct ExtSymbolStore {
    /// target → name → owners
    pub(crate) owners_by_target: HashMap<SymbolKey, OwnersBySymbolName>,
    /// owner → target → name → section → [variable keys]
    symbols_by_owner: HashMap<SymbolKey, DeclsByTarget>,
}

impl ExtSymbolStore {
    pub fn new() -> Self {
        Self {
            owners_by_target: HashMap::new(),
            symbols_by_owner: HashMap::new(),
        }
    }

    pub fn add(&mut self, target: SymbolKey, owner: SymbolKey, name: OYarn, section: u32, variable: SymbolKey) {
        self.owners_by_target
            .entry(target).or_default()
            .entry(name.clone()).or_insert_with(WeakSet::new)
            .insert(owner);

        self.symbols_by_owner
            .entry(owner).or_default()
            .entry(target).or_default()
            .entry(name).or_default()
            .entry(section).or_default()
            .push(variable);
    }

    pub fn remove(&mut self, key: SymbolKey) {
        // As target
        self.owners_by_target.remove(&key);
        // As owner
        self.symbols_by_owner.remove(&key);
        // As target in symbols_by_owner
        for decl in self.symbols_by_owner.values_mut() {
            decl.remove(&key);
        }
        // owner in owner_by_target handled by the weakset
    }

    // @arena: former get_decl_ext_symbol
    // Gets the symbol (`name`) injected by `owner` into `target`
    pub fn get(&self, owner: SymbolKey, target: SymbolKey, name: &str) -> Vec<SymbolKey> {
        let Some(decl_ext_symbols ) = self.symbols_by_owner.get(&owner) else {
            return vec![];
        };
        let mut result = vec![];
        if let Some(object_decl_symbols) = decl_ext_symbols.get(&target) {
            if let Some(symbols) = object_decl_symbols.get(name) {
                for end_symbols in symbols.values() {
                    //TODO actually we don't take position into account, but can we really?
                    result.extend(end_symbols);
                }
            }
        }
        result
    }
}

impl SymbolTable {


    // @arena: This used to be a method in each Symbol variant
    pub fn get_ext_symbol(&self, target: SymbolKey, name: &str) -> Vec<SymbolKey> {
        let Some(ext_symbols) = self.ext_symbols.owners_by_target.get(&target) else {
            return vec![];
        };

        let mut result = vec![];
        if let Some(owners) = ext_symbols.get(name) {
            for owner in owners.iter_valid(|&k| self.contains_key(k)) {
                result.extend(self.ext_symbols.get(owner, target, name));
            }
        }
        result
    }

}
