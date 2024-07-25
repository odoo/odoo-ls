use lsp_types::Diagnostic;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use weak_table::PtrWeakHashSet;

use crate::constants::BuildStatus;
use crate::core::localized_symbol::LocalizedSymbol;
use crate::core::symbol_location::SectionRange;

use super::symbol::MainSymbol;


#[derive(Debug)]
pub struct ClassSymbol {
    pub bases: PtrWeakHashSet<Weak<RefCell<LocalizedSymbol>>>,
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others and stored on FileInfo
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, Rc<RefCell<MainSymbol>>>,
}

impl ClassSymbol {

    pub fn inherits(&self, base: &Rc<RefCell<LocalizedSymbol>>, checked: &mut Option<PtrWeakHashSet<Weak<RefCell<LocalizedSymbol>>>>) -> bool {
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