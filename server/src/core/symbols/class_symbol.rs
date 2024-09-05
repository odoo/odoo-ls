use lsp_types::Diagnostic;
use ruff_text_size::TextRange;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use weak_table::PtrWeakHashSet;

use crate::constants::BuildStatus;
use crate::core::model::ModelData;

use super::symbol::Symbol;
use super::symbol_mgr::{SectionRange, SymbolMgr};


#[derive(Debug)]
pub struct ClassSymbol {
    pub name: String,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub bases: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub range: TextRange,
    pub _model: Option<ModelData>,

    //Trait SymbolMgr
    //--- Body symbols
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<String, Vec<Rc<RefCell<Symbol>>>>,
}

impl ClassSymbol {

    pub fn new(name: String, range: TextRange, is_external: bool) -> Self {
        let mut res = Self {
            name,
            is_external,
            weak_self: None,
            parent: None,
            range,
            ast_indexes: vec![],
            doc_string: None,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            bases: PtrWeakHashSet::new(),
            _model: None,
        };
        res._init_symbol_mgr();
        res
    }

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
            if b.borrow().as_class_sym().inherits(&base, checked) {
                return true;
            }
        }
        false
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

}