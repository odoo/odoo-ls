use crate::core::symbols::symbol_keys::{SymbolKey, VariableKey};
use crate::oyarn;
use crate::{
    constants::OYarn, core::symbols::storage::SymbolTable, weak_collections::WeakSet,
};
use crate::utils::HashMap;

/// section index → [variable keys] (Strong keys stored here)
type SectionSymbols = HashMap<u32, Vec<VariableKey>>;
/// name → section symbols
type NamedSectionSymbols = HashMap<OYarn, SectionSymbols>;
/// target/host → named section symbols
type DeclsByTarget = HashMap<SymbolKey, NamedSectionSymbols>;
/// name → set of owners
type OwnersBySymbolName = HashMap<OYarn, WeakSet<SymbolKey>>;

///   - `owners_by_target[target][name] → WeakSet<owner keys>`
///   - `symbols_by_owner[owner][target][name][section] → Vec<variable keys>`
#[derive(Debug)]
pub struct ExtSymbolStore {
    /// target → name → owners
    owners_by_target: HashMap<SymbolKey, OwnersBySymbolName>,
    /// owner → target → name → section → [variable keys]
    /// They are all injected by the ARCH_EVAL of the owner, not ARCH, as we need the evaluation of the target
    symbols_by_owner: HashMap<SymbolKey, DeclsByTarget>,
}

impl ExtSymbolStore {
    pub fn new() -> Self {
        Self {
            owners_by_target: HashMap::default(),
            symbols_by_owner: HashMap::default(),
        }
    }

    pub fn add(&mut self, target: SymbolKey, owner: SymbolKey, name: &str, section: u32, variable: VariableKey) {
        let name = oyarn!("{}", name);
        self.owners_by_target
            .entry(target).or_default()
            .entry(name.clone()).or_default()
            .insert(owner);

        self.symbols_by_owner
            .entry(owner).or_default()
            .entry(target).or_default()
            .entry(name).or_default()
            .entry(section).or_default()
            .push(variable);
    }

    /// Unregister the symbols injected by `owner`, and returns them (strong keys), for later
    /// removal from the symbol table
    /// Do not call directly, rather use symbolTable.remove_ext_symbols(owner)
    pub(super) fn remove_owned(&mut self, owner: SymbolKey) -> Vec<VariableKey> {
        let mut removed = vec![];
        if let Some(decls) = self.symbols_by_owner.remove(&owner) {
            for named in decls.into_values() {
                for sections in named.into_values() {
                    removed.extend(sections.into_values().flatten());
                }
            }
        }
        // owner in owners_by_target handled by the weakset
        removed
    }

    /// Considers that the giving key will be deleted, so remove everything related to it (owner or target)
    /// Returns the variable keys (strong keys) removed in the process, for
    /// later removal from the symbol table
    /// Do not call directly, rather use symbolTable.remove_ext_symbols(key)
    pub(super) fn remove(&mut self, key: SymbolKey) -> Vec<VariableKey> {
        // key as owner
        let mut orphaned = self.remove_owned(key);

        // key as target
        self.owners_by_target.remove(&key);
        // key as target in symbols_by_owner
        for decl in self.symbols_by_owner.values_mut() {
            if let Some(named) = decl.remove(&key) {
                for sections in named.into_values() {
                    orphaned.extend(sections.into_values().flatten());
                }
            }
        }
        orphaned
    }

    // Gets the symbol (`name`) injected by `owner` into `target`
    pub fn get(&self, owner: SymbolKey, target: SymbolKey, name: &str) -> Vec<VariableKey> {
        let Some(decl_ext_symbols ) = self.symbols_by_owner.get(&owner) else {
            return vec![];
        };
        let mut result = vec![];
        if let Some(object_decl_symbols) = decl_ext_symbols.get(&target)
            && let Some(symbols) = object_decl_symbols.get(name) {
                for end_symbols in symbols.values() {
                    //TODO actually we don't take position into account, but can we really?
                    result.extend(end_symbols);
                }
            }
        result
    }
}

impl SymbolTable {

    pub fn get_ext_symbol(&self, target: SymbolKey, name: &str) -> Vec<VariableKey> {
        let Some(ext_symbols) = self.ext_symbols.owners_by_target.get(&target) else {
            return vec![];
        };

        let mut result = vec![];
        if let Some(owners) = ext_symbols.get(name) {
            for owner in owners.iter_valid(self) {
                result.extend(self.ext_symbols.get(owner, target, name));
            }
        }
        result
    }

    /// Returns all the ext symbols injected into `target` whose name starts with `name_prefix`, grouped by name
    pub fn get_all_ext_symbols(&self, target: SymbolKey, name_prefix: &str) -> HashMap<OYarn, Vec<VariableKey>> {
        let mut result: HashMap<OYarn, Vec<VariableKey>> = HashMap::default();
        let Some(ext_symbols) = self.ext_symbols.owners_by_target.get(&target) else {
            return result;
        };
        for (name, owners) in ext_symbols.iter() {
            if !name.starts_with(name_prefix) {
                continue;
            }
            let symbols: Vec<VariableKey> = owners.iter_valid(self)
                .flat_map(|owner| self.ext_symbols.get(owner, target, name))
                .collect();
            if !symbols.is_empty() {
                result.entry(name.clone()).or_default().extend(symbols);
            }
        }
        result
    }

}
