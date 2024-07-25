use crate::{constants::SymType, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, rc::Weak};

use super::symbol::MainSymbol;

#[derive(Debug)]
pub struct RootSymbol {
    pub sys_path: Vec<String>, //sys path are stored in paths too, but this list identifies them
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            sys_path: vec![],
            weak_self: None,
            parent: None,
        }
    }

    pub fn add_symbol(&self, session: &mut SessionInfo, symbol: &mut RefMut<MainSymbol>) {
        match symbol.get_type() {
            SymType::FILE | SymType::PACKAGE => {
                for path in symbol.paths.iter() {
                    for sys_p in self.sys_path.iter() {
                        if sys_p.is_empty() {
                            continue;
                        }
                        if path.starts_with(sys_p) {
                            symbol.is_external = true;
                            return;
                        }
                    }
                    for stub in session.sync_odoo.stubs_dirs.iter() {
                        if path.starts_with(stub) || path.starts_with(&session.sync_odoo.stdlib_dir) {
                            symbol.is_external = true;
                            return;
                        }
                    }
                }
            },
            _ => {}
        }
    }

}