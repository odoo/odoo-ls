use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use crate::constants::*;
use crate::core::symbol::Symbol;

#[derive(Debug)]
pub struct Evaluation {
    symbol: Weak<Mutex<Symbol>>,
    pub instance: bool,
    pub context: HashMap<String, bool>,
    pub value: Option<String>,
    _internal_hold_symbol: Option<Arc<Mutex<Symbol>>>,
}

impl Evaluation {

    pub fn eval_from_symbol(symbol: &Arc<Mutex<Symbol>>) -> Evaluation{
        let mut instance = false;
        if [SymType::VARIABLE, SymType::PRIMITIVE].contains(&symbol.lock().unwrap().sym_type) {
            instance = true
        }
        Evaluation {
            symbol: Arc::downgrade(symbol),
            instance: instance,
            context: HashMap::new(),
            value: None,
            _internal_hold_symbol: None,
        }
    }

    pub fn get_symbol(&self) -> Weak<Mutex<Symbol>> { //TODO evaluate context
        self.symbol.clone()
    }

}