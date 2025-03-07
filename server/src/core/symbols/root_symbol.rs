use byteyarn::{yarn, Yarn};
use serde_json::json;

use crate::{constants::BuildSteps, core::entry_point::EntryPoint, threads::SessionInfo, S};
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct RootSymbol {
    pub name: Yarn,
    pub entry_point: Option<Rc<RefCell<EntryPoint>>>,
    pub paths: Vec<String>,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub module_symbols: HashMap<Yarn, Rc<RefCell<Symbol>>>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            name: yarn!("Root"),
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

    pub fn to_json(&self) -> serde_json::Value {
        let module_sym: Vec<serde_json::Value> = self.module_symbols.values().map(|sym| {
            json!({
                "name": sym.borrow().name().clone(),
                "type": sym.borrow().typ().to_string(),
            })
        }).collect();
        json!({
            "type": SymType::ROOT.to_string(),
            "module_symbols": module_sym,
        })
    }

}
