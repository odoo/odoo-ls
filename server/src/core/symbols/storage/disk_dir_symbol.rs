
use std::path::Path;
use crate::core::symbols::storage::FileSystemSymbolParent;
use crate::core::symbols::symbol_keys::JsFileKey;
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
    parent: FileSystemSymbolParent,
    pub(super) fs_symbols: HashMap<OYarn, SymbolKey>,
    pub(super) js_symbols: HashMap<String, JsFileKey>,
}


impl DiskDirSymbol {

    pub fn new(name: &str, path: &str, parent: FileSystemSymbolParent, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            path: Path::new(path).sanitize(),
            is_external,
            parent,
            in_workspace: false,
            fs_symbols: HashMap::default(),
            js_symbols: HashMap::default(),
        }
    }

    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.fs_symbols
    }

    pub fn parent(&self) -> FileSystemSymbolParent {
        self.parent
    }
}
