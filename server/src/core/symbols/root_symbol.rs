use crate::{constants::BuildSteps, core::entry_point::EntryPoint, threads::SessionInfo, S};
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct RootSymbol {
    pub name: String,
    pub entry_point: Option<Rc<RefCell<EntryPoint>>>,
    pub paths: Vec<String>,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            name: S!("Root"),
            paths: vec![],
            weak_self: None,
            entry_point: None,
            parent: None,
            module_symbols: HashMap::new(),
        }
    }

    pub fn add_file(&mut self, file: &Rc<RefCell<Symbol>>) {
        file.borrow_mut().set_is_external(true);
        self.module_symbols.insert(file.borrow().name().clone(), file.clone());
    }

}
