use crate::{constants::OYarn, core::{entry_point::EntryPoint, symbols::symbol_keys::SymbolKey}, oyarn};
use std::{cell::RefCell, rc::Rc};
use crate::utils::HashMap;

#[derive(Debug)]
pub struct RootSymbol {
    pub name: OYarn,
    entry_point: Rc<RefCell<EntryPoint>>,

    // child symbols (no parent)
    pub(super) fs_symbols: HashMap<OYarn, SymbolKey>,
}

impl RootSymbol {

    pub fn new(entry_point: Rc<RefCell<EntryPoint>>) -> Self {
        Self {
            name: oyarn!("Root"),
            entry_point,
            fs_symbols: HashMap::default(),
        }
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.fs_symbols
    }
 
    pub fn entry_point(&self) -> &Rc<RefCell<EntryPoint>> {
        &self.entry_point
    }

}
