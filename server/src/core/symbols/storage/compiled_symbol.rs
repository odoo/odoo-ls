use crate::utils::HashMap;


use crate::{constants::OYarn, oyarn};
use crate::core::symbols::symbol_keys::SymbolKey;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub path: String,
    
    // parent / child symbols
    parent: SymbolKey,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

impl CompiledSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            is_external,
            path: path.to_string(),
            module_symbols: HashMap::default(),
            parent,
        }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }
    
    pub fn children(&self) -> Vec<SymbolKey> {
        self.module_symbols.values().copied().collect()
    }

}
