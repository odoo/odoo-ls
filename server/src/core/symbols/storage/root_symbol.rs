use crate::{constants::OYarn, core::{entry_point::EntryPoint, symbols::symbol_keys::SymbolKey}, oyarn};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Debug)]
pub struct RootSymbol {
    pub name: OYarn,
    entry_point: Rc<RefCell<EntryPoint>>,

    // child symbols (no parent)
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

impl RootSymbol {

    pub fn new(entry_point: Rc<RefCell<EntryPoint>>) -> Self {
        Self {
            name: oyarn!("Root"),
            entry_point,
            module_symbols: HashMap::new(),
        }
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.module_symbols
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        self.module_symbols.values().copied().collect()
    }

    pub fn entry_point(&self) -> &Rc<RefCell<EntryPoint>> {
        &self.entry_point
    }

}
