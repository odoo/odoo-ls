use std::collections::HashMap;

use ruff_text_size::TextRange;

use crate::{constants::{OYarn, PackageType, SymType}, core::symbols::{symbol_mgr::SymbolMgr, symbol_table::{SymbolKey, SymbolTable}, variable_symbol::VariableSymbol}, weak_hash_set::WeakSet};
use crate::core::symbols::symbol_table::ContainsKey;

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
        // @arena: assumes owner as valid key (formerly an strong Rc)
    // @arena TODO: fix this weird API (take &str instead of OYarn)
    pub fn add_new_ext_symbol(
        &mut self,
        target: SymbolKey,
        name: OYarn,
        range: &TextRange,
        owner: SymbolKey,
    ) -> SymbolKey {
        let target_sym = self.get_symbol_view(target).expect("valid key");
        // validate target can host an external symbol
        if !matches!(target_sym.typ(),
            SymType::FILE | SymType::PACKAGE(PackageType::MODULE)
                | SymType::PACKAGE(PackageType::PYTHON_PACKAGE)
                | SymType::CLASS | SymType::FUNCTION | SymType::NAMESPACE
        ) {
            panic!("Impossible to add an external symbol to a {}", target_sym.typ());
        }
        let variable_symbol = VariableSymbol::new(
            name.clone(),
            target,
            range.clone(),
            target_sym.is_external(),
        );
        let variable_key: SymbolKey = self.variables.insert(variable_symbol).into();
        let section = self.get_section_for_key(owner, range.start().to_u32());

        self.ext_symbols.add(target, owner, name, section, variable_key);
        variable_key
    }

    // @arena: assumes owner as valid key (formerly self on a Symbol)
    /* used by add_new_ext_symbol. Do not call directly */
    fn get_section_for_key(&self, owner: SymbolKey, position: u32) -> u32 {
        match owner {
            SymbolKey::File(f) => self.files[f].get_section_for(position).index,
            SymbolKey::Module(m) => self.modules[m].get_section_for(position).index,
            SymbolKey::PythonPackage(p) => self.python_packages[p].get_section_for(position).index,
            SymbolKey::Class(c) => self.classes[c].get_section_for(position).index,
            SymbolKey::Function(f) => self[f].get_section_for(position).index,
            _ => panic!(
                "Impossible to add a declaration of external symbol to a {}",
                self.get_symbol_view(owner).unwrap().typ()
            ),
        }
    }

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
