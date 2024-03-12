use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;
use weak_table::PtrWeakHashSet;

use crate::core::symbol::Symbol;

pub struct Model {
    name: String,
    symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
}

impl Model {
    pub fn new(name: String, symbol: Rc<RefCell<Symbol>>) -> Self {
        let mut res = Self {
            name,
            symbols: PtrWeakHashSet::new(),
        };
        res.symbols.insert(symbol);
        res
    }
}
