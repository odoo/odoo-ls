use crate::{constants::OYarn, core::symbols::symbol_keys::{EntryPointKey, SymbolKey}, oyarn};
use crate::utils::HashMap;

#[derive(Debug)]
pub struct RootSymbol {
    pub name: OYarn,
    entry_point: EntryPointKey,

    // child symbols (no parent)
    pub(super) fs_symbols: HashMap<OYarn, SymbolKey>,
}

impl RootSymbol {

    pub fn new(entry_point: EntryPointKey) -> Self {
        Self {
            name: oyarn!("Root"),
            entry_point,
            fs_symbols: HashMap::default(),
        }
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.fs_symbols
    }

    pub fn entry_point(&self) -> EntryPointKey {
        self.entry_point
    }

}
