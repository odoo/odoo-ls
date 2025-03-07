use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use byteyarn::{yarn, Yarn};
use serde_json::json;

use crate::constants::SymType;

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

    pub fn to_json(&self) -> serde_json::Value {
        let module_sym: Vec<serde_json::Value> = self.module_symbols.values().map(|sym| {
            json!({
                "name": sym.borrow().name().clone(),
                "type": sym.borrow().typ().to_string(),
            })
        }).collect();
        json!({
            "type": SymType::COMPILED.to_string(),
            "path": self.path,
            "is_external": self.is_external,
            "module_symbols": module_sym,
        })
    }

}