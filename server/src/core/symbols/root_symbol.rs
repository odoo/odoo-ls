use crate::{constants::SymType, core::odoo::SyncOdoo, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, collections::HashMap, rc::{Rc, Weak}};

use super::symbol::MainSymbol;

#[derive(Debug)]
pub struct RootSymbol {
    pub paths: Vec<String>,
    pub sys_path: Vec<String>, //sys path are stored in paths too, but this list identifies them
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub module_symbols: HashMap<String, Rc<RefCell<MainSymbol>>>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            paths: vec![],
            sys_path: vec![],
            weak_self: None,
            parent: None,
            module_symbols: HashMap::new(),
        }
    }

    pub fn add_file(&mut self, session: &mut SessionInfo, file: Rc<RefCell<MainSymbol>>) {
        for path in file.borrow().paths().iter() {
            for sys_p in self.sys_path.iter() {
                if sys_p.is_empty() {
                    continue;
                }
                if path.starts_with(sys_p) {
                    file.borrow_mut().set_is_external(true);
                    return;
                }
            }
            for stub in session.sync_odoo.stubs_dirs.iter() {
                if path.starts_with(stub) || path.starts_with(&session.sync_odoo.stdlib_dir) {
                    file.borrow_mut().set_is_external(true);
                    return;
                }
            }
        }
        self.module_symbols.insert(file.borrow().name().clone(), file.clone());
    }

}