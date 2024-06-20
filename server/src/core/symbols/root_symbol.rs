use crate::threads::SessionInfo;
use crate::{constants::SymType, core::symbol::Symbol};
use std::cell::RefMut;

#[derive(Debug)]
pub struct RootSymbol {
    pub sys_path: Vec<String>, //sys path are stored in paths too, but this list identifies them
}

impl RootSymbol {

    pub fn add_symbol(&self, session: &mut SessionInfo, _self_symbol: &Symbol, symbol: &mut RefMut<Symbol>) {
        match symbol.sym_type {
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