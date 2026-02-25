use std::collections::{HashMap, HashSet};

use ruff_text_size::TextRange;

use crate::{constants::{OYarn, PackageType, SymType}, core::symbols::{package_symbol::PackageSymbol, symbol_mgr::SymbolMgr, symbol_table::{SymbolKey, SymbolTable}, variable_symbol::VariableSymbol}};

/// section index → [variable keys]                                                                  
type SectionSymbols = HashMap<u32, Vec<SymbolKey>>;                                                  
/// name → section symbols
type NamedSectionSymbols = HashMap<OYarn, SectionSymbols>;                                           
/// target/host → named section symbols
type DeclExtSymbols = HashMap<SymbolKey, NamedSectionSymbols>;
/// name → set of owner keys
type ExtSymbolOwners = HashMap<OYarn, HashSet<SymbolKey>>;

#[derive(Debug)]
pub struct ExtSymbolStore {
    /// formerly ext_symbols
    /// target → name → owners
    pub(crate) owners: HashMap<SymbolKey, ExtSymbolOwners>,
    /// formerly decl_ext_symbols
    /// owner → target → name → section → [variable keys]
    declarations: HashMap<SymbolKey, DeclExtSymbols>,
}

impl ExtSymbolStore {
    pub fn new() -> Self {
        Self {
            owners: HashMap::new(),
            declarations: HashMap::new(),
        }
    }

    pub fn add(&mut self, target: SymbolKey, owner: SymbolKey, name: OYarn, section: u32, variable: SymbolKey) {
        self.owners
            .entry(target).or_default()
            .entry(name.clone()).or_default()
            .insert(owner);

        self.declarations
            .entry(owner).or_default()
            .entry(target).or_default()
            .entry(name).or_default()
            .entry(section).or_default()
            .push(variable);
    }

    // @arena: former get_decl_ext_symbol
    // Gets the symbol (`name`) injected by `owner` into `target`
    pub fn get(&self, owner: SymbolKey, target: SymbolKey, name: &OYarn) -> Vec<SymbolKey> {
        let Some(decl_ext_symbols ) = self.declarations.get(&owner) else {
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
        let target_sym = self.get_symbol(target).expect("valid key");
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
            SymbolKey::Package(p) => {
                match &self.packages[p] {
                    PackageSymbol::Module(m) => m.get_section_for(position).index,
                    PackageSymbol::PythonPackage(p) => p.get_section_for(position).index,
                }
            },
            SymbolKey::Class(c) => self.classes[c].get_section_for(position).index,
            SymbolKey::Function(f) => self.functions[f].get_section_for(position).index,
            _ => panic!(
                "Impossible to add a declaration of external symbol to a {}",
                self.get_symbol(owner).unwrap().typ()
            ),
        }
    }

        // @arena: This used to be a method in each Symbol variant
    pub fn get_ext_symbol(&self, target: SymbolKey, name: &OYarn) -> Vec<SymbolKey> {
        let Some(ext_symbols) = self.ext_symbols.owners.get(&target) else {
            return vec![];
        };

        let mut result = vec![];
        if let Some(owners) = ext_symbols.get(name) {
            for &owner in owners {
                if !self.contains_key(owner) {
                    // @arena: Equivalent of iterating on a PtrWeakHashSet, which cleans up expired weaks
                    // todo: remove key from ExtSymbolStore
                    continue;
                }
                result.extend(self.ext_symbols.get(owner, target, name));
            }
        }
        result
    }

}

