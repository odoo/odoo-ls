use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::symbol::Symbol;

#[derive(Debug)]
pub struct Evaluation {
    symbol: Weak<RefCell<Symbol>>,
    pub instance: bool,
    pub context: HashMap<String, bool>,
    pub value: Option<String>,
    _internal_hold_symbol: Option<Rc<RefCell<Symbol>>>,
}

impl Evaluation {

    pub fn eval_from_symbol(symbol: &Rc<RefCell<Symbol>>) -> Evaluation{
        let mut instance = false;
        if [SymType::VARIABLE, SymType::PRIMITIVE].contains(&symbol.borrow_mut().sym_type) {
            instance = true
        }
        Evaluation {
            symbol: Rc::downgrade(symbol),
            instance: instance,
            context: HashMap::new(),
            value: None,
            _internal_hold_symbol: None,
        }
    }

    pub fn get_symbol(&self) -> Weak<RefCell<Symbol>> { //TODO evaluate context
        self.symbol.clone()
    }

}