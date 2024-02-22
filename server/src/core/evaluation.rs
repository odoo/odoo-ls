use rustpython_parser::ast::{Expr};
use rustpython_parser::text_size::TextRange;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::symbol::Symbol;

#[derive(Debug, Default)]
pub struct Evaluation {
    symbol: Weak<RefCell<Symbol>>,
    pub instance: bool,
    pub context: HashMap<String, bool>,
    pub value: Option<Expr<TextRange>>,
    _internal_hold_symbol: Option<Rc<RefCell<Symbol>>>,
}

impl Evaluation {

    pub fn eval_from_symbol(symbol: &Rc<RefCell<Symbol>>) -> Evaluation{
        let mut instance = false;
        if [SymType::VARIABLE, SymType::CONSTANT].contains(&symbol.borrow_mut().sym_type) {
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

    // eval an ast expression that represent the evaluation of a symbol.
    // For example, in a= 1+2, it will create the evaluation of 1+2 to be stored on a
    pub fn eval_from_ast(ast: &Expr<TextRange>, parent: Rc<RefCell<Symbol>>) -> Evaluation {
        let mut res = Evaluation::default();
        match ast {
            Expr::Constant(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_c".to_string(), SymType::CONSTANT))));
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                res.value = Some(Expr::Constant(expr.clone()));
            }
            _ => {}
        }
        res
    }

    pub fn get_symbol(&self) -> Weak<RefCell<Symbol>> { //TODO evaluate context
        self.symbol.clone()
    }

}