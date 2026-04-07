use crate::{constants::OYarn, core::{entry_point::EntryPoint, symbols::symbol_keys::SymbolKey}, oyarn};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Debug)]
pub struct RootSymbol {
    pub name: OYarn,
    pub entry_point: Option<Rc<RefCell<EntryPoint>>>,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            name: oyarn!("Root"),
            entry_point: None,
            module_symbols: HashMap::new(),
        }
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.module_symbols
    }

}
