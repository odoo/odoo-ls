use byteyarn::Yarn;
use ruff_python_ast::AtomicNodeIndex;
use ruff_text_size::{TextRange, TextSize};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use weak_table::{PtrWeakHashSet, PtrWeakKeyHashMap};

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
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub range: TextRange,
    pub body_range: TextRange,
    pub _model: Option<ModelData>,
    pub noqas: NoqaInfo,
    pub(crate) _is_field_class: Rc<RefCell<Option<bool>>>, //cache, do not call directly, use is_field_class() method instead

    //Trait SymbolMgr
    //--- Body symbols
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, PtrWeakHashSet<Weak<RefCell<Symbol>>>>,
    pub decl_ext_symbols: PtrWeakKeyHashMap<Weak<RefCell<Symbol>>, HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>>
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
            doc_string: None,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            decl_ext_symbols: PtrWeakKeyHashMap::new(),
            bases: vec![],
            _model: None,
            noqas: NoqaInfo::None,
            _is_field_class: Rc::new(RefCell::new(None)),
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

    pub fn get_ext_symbol(&self, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result = vec![];
        if let Some(owners) = self.ext_symbols.get(name) {
            for owner in owners.iter() {
                let owner = owner.borrow();
                result.extend(owner.get_decl_ext_symbol(&self.weak_self.as_ref().unwrap().upgrade().unwrap(), name));
            }
        }
        result
    }

    pub fn get_decl_ext_symbol(&self, symbol: &Rc<RefCell<Symbol>>, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result = vec![];
        if let Some(object_decl_symbols) = self.decl_ext_symbols.get(symbol) {
            if let Some(symbols) = object_decl_symbols.get(name) {
                for end_symbols in symbols.values() {
                    //TODO actually we don't take position into account, but can we really?
                    result.extend(end_symbols.iter().map(|s| s.clone()));
                }
            }
        }
        result
    }

}
