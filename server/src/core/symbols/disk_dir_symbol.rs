
use std::{collections::HashMap, path::PathBuf};

use crate::{constants::OYarn, core::symbols::symbol_table::SymbolKey, oyarn, utils::PathSanitizer};


/*
DiskDir symbol represent a directory on disk we didn't parse yet. So it can either be a namespace or a package later.
*/
#[derive(Debug)]
pub struct DiskDirSymbol {
    pub name: OYarn,
    pub path: String,
    // @todo: this name is confusing. Maybe just "children"?
    pub module_symbols: HashMap<OYarn, SymbolKey>,
    pub is_external: bool,
    // pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<SymbolKey>,
    pub in_workspace: bool,
}

impl DiskDirSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            path: PathBuf::from(path).sanitize(),
            is_external,
            // weak_self: None,
            parent: None,
            in_workspace: false,
            module_symbols: HashMap::new()
        }
    }

    pub fn add_file(&mut self, file: SymbolKey, name: &OYarn) {
        self.module_symbols.insert(name.clone(), file);
    }

    /*pub fn load(sesion: &mut SessionInfo, dir: &Rc<RefCell<Symbol>>) -> Rc<RefCell<Symbol>> {
        let path = dir.borrow().as_disk_dir_sym().path.clone();
    }*/
}