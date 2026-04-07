use std::collections::HashMap;


use crate::{constants::OYarn, oyarn};
use crate::core::symbols::symbol_keys::SymbolKey;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub path: String,
    parent: SymbolKey,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
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

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

}
