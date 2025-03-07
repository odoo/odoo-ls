use byteyarn::{yarn, Yarn};
use serde_json::json;
use weak_table::PtrWeakHashSet;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::{Rc, Weak}};

use crate::{constants::SymType, threads::SessionInfo, utils::PathSanitizer};

use super::symbol::Symbol;

/*
DiskDir symbol represent a directory on disk we didn't parse yet. So it can either be a namespace or a package later.
*/
#[derive(Debug)]
pub struct DiskDirSymbol {
    pub name: Yarn,
    pub path: String,
    pub module_symbols: HashMap<Yarn, Rc<RefCell<Symbol>>>,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub in_workspace: bool,
}

impl DiskDirSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
            name: yarn!("{}", name),
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

    pub fn to_json(&self) -> serde_json::Value {
        let module_sym: Vec<serde_json::Value> = self.module_symbols.values().map(|sym| {
            json!({
                "name": sym.borrow().name().clone(),
                "type": sym.borrow().typ().to_string(),
            })
        }).collect();
        json!({
            "type": SymType::DISK_DIR.to_string(),
            "path": self.path,
            "is_external": self.is_external,
            "in_workspace": self.in_workspace,
            "module_symbols": module_sym,
        })
    }
}