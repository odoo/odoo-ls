use crate::{constants::SymType, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, collections::HashMap, rc::{Rc, Weak}};

use super::symbol::MainSymbol;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: String,
    pub is_external: bool,
    pub path: String,
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub module_symbols: HashMap<String, Rc<RefCell<MainSymbol>>>,
}

impl CompiledSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
            name,
            is_external,
            weak_self:None,
            path,
            module_symbols: HashMap::new(),
            parent: None,
        }
    }

    pub fn add_compiled(&mut self, compiled: &Rc<RefCell<MainSymbol>>) {
        self.module_symbols.insert(compiled.borrow().name().clone(), compiled.clone());
    }

}