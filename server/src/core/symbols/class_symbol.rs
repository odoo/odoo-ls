
use ruff_text_size::TextRange;
use tower_lsp::lsp_types::Diagnostic;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use weak_table::PtrWeakHashSet;

use crate::core::symbol::Symbol;


#[derive(Debug)]
pub struct ClassSymbol {
    pub bases: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others and stored on FileInfo
}

impl ClassSymbol {

    pub fn inherits(&self, base: &Rc<RefCell<Symbol>>, checked: &mut Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>) -> bool {
        if checked.is_none() {
            *checked = Some(PtrWeakHashSet::new());
        }
        for b in self.bases.iter() {
            if Rc::ptr_eq(&b, base) {
                return true;
            }
            if checked.as_ref().unwrap().contains(&b) {
                return false;
            }
            checked.as_mut().unwrap().insert(b.clone());
            if b.borrow()._class.as_ref().unwrap().inherits(&base, checked) {
                return true;
            }
        }
        false
    }

}