use crate::core::symbols::storage::FileSystemSymbolParent;
use crate::utils::HashMap;

use crate::{constants::OYarn, oyarn};
use crate::core::symbols::symbol_keys::SymbolKey;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub path: String,
    
    // parent / child symbols
    parent: FileSystemSymbolParent,
    pub(super) fs_symbols: HashMap<OYarn, SymbolKey>,
}

impl CompiledSymbol {

    pub fn new(name: &str, path: &str, parent: FileSystemSymbolParent, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            is_external,
            path: path.to_string(),
            fs_symbols: HashMap::default(),
            parent,
        }
    }

    pub fn parent(&self) -> FileSystemSymbolParent {
        self.parent
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.fs_symbols
    }

}
