use lsp_types::Diagnostic;
use ruff_text_size::TextRange;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use weak_table::PtrWeakHashSet;

use crate::constants::BuildStatus;

use super::symbol::MainSymbol;
use super::symbol_mgr::SectionRange;


#[derive(Debug)]
pub struct ClassSymbol {
    pub name: String,
    pub is_external: bool,
    pub bases: PtrWeakHashSet<Weak<RefCell<MainSymbol>>>,
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others and stored on FileInfo
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub range: TextRange,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<MainSymbol>>>>>,
}

impl ClassSymbol {

    pub fn inherits(&self, base: &Rc<RefCell<MainSymbol>>, checked: &mut Option<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>) -> bool {
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