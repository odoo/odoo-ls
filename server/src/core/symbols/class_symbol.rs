use byteyarn::Yarn;
use ruff_text_size::{TextRange, TextSize};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use weak_table::PtrWeakHashSet;

use crate::constants::{OYarn, SymType};
use crate::core::file_mgr::NoqaInfo;
use crate::core::model::ModelData;
use crate::threads::SessionInfo;
use crate::{Sy, S};

use super::symbol::Symbol;
use super::symbol_mgr::{SectionRange, SymbolMgr};


#[derive(Debug)]
pub struct ClassSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub bases: Vec<Weak<RefCell<Symbol>>>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub range: TextRange,
    pub body_range: TextRange,
    pub _model: Option<ModelData>,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    //--- Body symbols
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>,
}

impl ClassSymbol {

    pub fn new(name: String, range: TextRange, body_start: TextSize, is_external: bool) -> Self {
        let mut res = Self {
            name: OYarn::from(name),
            is_external,
            weak_self: None,
            parent: None,
            range,
            body_range: TextRange::new(body_start, range.end()),
            ast_indexes: vec![],
            doc_string: None,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            bases: vec![],
            _model: None,
            noqas: NoqaInfo::None,
        };
        res._init_symbol_mgr();
        res
    }

    pub fn inherits(&self, base: &Rc<RefCell<Symbol>>, checked: &mut Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>) -> bool {
        if checked.is_none() {
            *checked = Some(PtrWeakHashSet::new());
        }
        for base_weak in self.bases.iter() {
            let b = match base_weak.upgrade(){
                Some(b) => b,
                None => continue
            };
            if Rc::ptr_eq(&b, base) {
                return true;
            }
            if checked.as_ref().unwrap().contains(&b) {
                return false;
            }
            checked.as_mut().unwrap().insert(b.clone());
            if b.borrow().as_class_sym().inherits(base, checked) {
                return true;
            }
        }
        false
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert(HashMap::new());
        let section_vec = sections.entry(section).or_insert(vec![]);
        section_vec.push(content.clone());
    }

    pub fn is_descriptor(&self) -> bool {
        for get_sym in self.get_content_symbol(Sy!("__get__"), u32::MAX).symbols.iter() {
            if get_sym.borrow().typ() == SymType::FUNCTION {
                return true;
            }
        }
        false
    }

}
