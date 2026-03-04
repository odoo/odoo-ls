use std::collections::HashMap;


use crate::{constants::OYarn, core::symbols::symbol_table::SymbolKey, oyarn};

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub path: String,
    // pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<SymbolKey>,
    pub module_symbols: HashMap<OYarn, SymbolKey>,
    // pub ext_symbols: HashMap<OYarn, PtrWeakHashSet<Weak<RefCell<Symbol>>>>,
}

impl CompiledSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            is_external,
            // weak_self: None,
            path: path.to_string(),
            module_symbols: HashMap::new(),
            // ext_symbols: HashMap::new(),
            parent: Some(parent),
        }
    }

    pub fn add_compiled(&mut self, compiled: SymbolKey, name: &str) {
        self.module_symbols.insert(oyarn!("{}", name), compiled);
    }

}