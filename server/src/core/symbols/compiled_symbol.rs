use std::collections::HashMap;


use crate::{constants::OYarn, core::symbols::symbol_table::SymbolKey, oyarn};

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub path: String,
    parent: SymbolKey,
    pub module_symbols: HashMap<OYarn, SymbolKey>,
}

impl CompiledSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            is_external,
            path: path.to_string(),
            module_symbols: HashMap::new(),
            parent,
        }
    }

    pub fn add_compiled(&mut self, compiled: SymbolKey, name: &str) {
        self.module_symbols.insert(oyarn!("{}", name), compiled);
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

}
