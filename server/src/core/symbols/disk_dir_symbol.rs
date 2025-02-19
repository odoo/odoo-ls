use weak_table::PtrWeakHashSet;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::{Rc, Weak}};

use crate::{threads::SessionInfo, utils::PathSanitizer};

use super::symbol::Symbol;

/*
DiskDir symbol represent a directory on disk we didn't parse yet. So it can either be a namespace or a package later.
*/
#[derive(Debug)]
pub struct DiskDirSymbol {
    pub name: String,
    pub path: String,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub in_workspace: bool,
}

impl DiskDirSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
            name,
            path: PathBuf::from(path).sanitize(),
            is_external,
            weak_self: None,
            parent: None,
            in_workspace: false,
            module_symbols: HashMap::new()
        }
    }

    pub fn add_file(&mut self, file: &Rc<RefCell<Symbol>>) {
        self.module_symbols.insert(file.borrow().name().clone(), file.clone());
    }

    /*pub fn load(sesion: &mut SessionInfo, dir: &Rc<RefCell<Symbol>>) -> Rc<RefCell<Symbol>> {
        let path = dir.borrow().as_disk_dir_sym().path.clone();
    }*/
}