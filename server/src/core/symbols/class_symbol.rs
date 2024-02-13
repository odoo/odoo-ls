use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::collections::HashSet;
use crate::core::symbol::Symbol;


#[derive(Debug)]
pub struct ClassSymbol {
    pub bases: HashSet<Weak<Mutex<Symbol>>>,
}

impl ClassSymbol {
    
    pub fn inherits(&self, symbol: MutexGuard<Symbol>, checked: &mut HashSet<Weak<Mutex<Symbol>>>) -> bool {
        // for base in self.bases.iter() {
        //     match base.upgrade() {
        //         Some(base) => {
        //             if base == symbol {
        //                 return true;
        //             }
        //             if checked.contains(&base) {
        //                 return false;
        //             }
        //             checked.add(base.clone());
        //             if base.inherits(symbol, checked) {
        //                 return true;
        //             }
        //         },
        //         None => {}
        //     }
        // }
        // checked.insert(Arc::downgrade(&symbol));
        // if symbol.sym_type == SymType::CLASS {
        //     self.bases.insert(Arc::downgrade(&symbol));
        //     for base in symbol.lock().unwrap().bases.iter() {
        //         self.inherits(base.lock().unwrap(), checked);
        //     }
        // }
        false
    }

}