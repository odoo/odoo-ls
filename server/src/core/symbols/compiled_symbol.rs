use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use byteyarn::{yarn, Yarn};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: Yarn,
    pub is_external: bool,
    pub path: String,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub module_symbols: HashMap<Yarn, Rc<RefCell<Symbol>>>,
}

impl CompiledSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
            name: Yarn::from_string(name),
            is_external,
            weak_self:None,
            path,
            module_symbols: HashMap::new(),
            parent: None,
        }
    }

    pub fn add_compiled(&mut self, compiled: &Rc<RefCell<Symbol>>) {
        self.module_symbols.insert(compiled.borrow().name().clone(), compiled.clone());
    }

}