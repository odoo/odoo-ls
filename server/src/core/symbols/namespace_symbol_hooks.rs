use std::{cell::RefCell, rc::Rc};

use crate::{threads::SessionInfo, Sy};

use super::symbol::{self, Symbol};



pub struct NamespaceSymbolHooks {}

impl NamespaceSymbolHooks {

    pub fn on_create(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>) {
        let symbol = symbol.borrow_mut();
        match symbol.name().as_str() {
            "odoo" => {
                if session.sync_odoo.full_version.as_str() >= "18.1" {
                    if symbol.get_main_entry_tree(session) == (vec![Sy!("odoo")], vec![]) {
                        // create _ and Command as ext_symbols
                        symbol.as_namespace_mut().ext_symbols.insert(Sy!("_"), symbol::Symbol::add_new_variable(session, symbol.clone(), Sy!("_")));
                    }
                }
            },
            _ => {}
        }
    }
}