use crate::{constants::BuildSteps, threads::SessionInfo, S};
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct RootSymbol {
    pub name: String,
    pub paths: Vec<String>,
    pub sys_path: Vec<String>, //sys path are stored in paths too, but this list identifies them
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            name: S!("Root"),
            paths: vec![],
            sys_path: vec![],
            weak_self: None,
            parent: None,
            module_symbols: HashMap::new(),
        }
    }

    pub fn add_file(&mut self, session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>) {
        let paths = file.borrow().paths().clone();
        self.module_symbols.insert(file.borrow().name().clone(), file.clone());
        for path in paths.iter() {
            for sys_p in self.sys_path.iter() {
                if sys_p.is_empty() || *sys_p == session.sync_odoo.config.odoo_path || session.sync_odoo.config.addons.contains(sys_p){
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
    }

}
