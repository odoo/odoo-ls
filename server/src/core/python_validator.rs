use std::rc::Rc;
use std::cell::RefCell;
use crate::core::symbol::Symbol;
use crate::core::odoo::SyncOdoo;

pub struct PythonValidator {

}

impl PythonValidator {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> Self {
        Self {
            
        }
    }

    pub fn validate(&self, odoo: &mut SyncOdoo) {
        println!("PythonValidator::validate");
    }
}