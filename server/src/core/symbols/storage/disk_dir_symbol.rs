
use std::{path::PathBuf};
use crate::utils::HashMap;

use crate::{constants::OYarn, core::symbols::symbol_keys::SymbolKey, oyarn, utils::PathSanitizer};


/*
DiskDir symbol represent a directory on disk we didn't parse yet. So it can either be a namespace or a package later.
*/
#[derive(Debug)]
pub struct DiskDirSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    pub in_workspace: bool,
    
    // parent / child symbols
    parent: SymbolKey,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

impl DiskDirSymbol {

    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            path: PathBuf::from(path).sanitize(),
            is_external,
            parent,
            in_workspace: false,
            module_symbols: HashMap::default()
        }
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.module_symbols
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }
    
    pub fn children(&self) -> Vec<SymbolKey> {
        self.module_symbols.values().copied().collect()
    }
    
    /*pub fn load(sesion: &mut SessionInfo, dir: &Rc<RefCell<Symbol>>) -> Rc<RefCell<Symbol>> {
        let path = dir.borrow().as_disk_dir_sym().path.clone();
    }*/
}
