use ruff_text_size::{TextRange, TextSize};
use std::collections::{HashMap, HashSet};
use std::cell::RefCell;

use crate::constants::OYarn;
use crate::core::file_mgr::NoqaInfo;
use crate::core::model::ModelData;
use crate::oyarn;
use crate::core::symbols::symbol_table::{ClassKey, SymbolKey, SymbolTable, Weak};

use super::symbol_mgr::{SectionRange, SymbolMgr};


#[derive(Debug)]
pub struct ClassSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub bases: Vec<Weak<ClassKey>>, // formely Vec<Weak<RefCell<Symbol>>>
    pub parent: SymbolKey,
    pub range: TextRange,
    pub body_range: TextRange,
    pub _model: Option<ModelData>,
    pub noqas: NoqaInfo,
    pub(crate) _is_field_class: RefCell<Option<bool>>, //cache, do not call directly, use is_field_class() method instead

    //Trait SymbolMgr
    //--- Body symbols
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>>>,
}

impl ClassSymbol {

    pub fn new(name: &str, parent: SymbolKey, range: TextRange, body_start: TextSize, is_external: bool) -> Self {
        let mut res = Self {
            name: oyarn!("{}", name),
            is_external,
            parent,
            range,
            body_range: TextRange::new(body_start, range.end()),
            doc_string: None,
            sections: vec![],
            symbols: HashMap::new(),
            bases: vec![],
            _model: None,
            noqas: NoqaInfo::None,
            _is_field_class: RefCell::new(None),
        };
        res._init_symbol_mgr();
        res
    }

    // @arena: search stop when a visited class is encountered, instead of just skipping it. Bug?
    pub fn inherits(symbol_table: &SymbolTable, class_key: ClassKey, base: ClassKey, checked: &mut Option<HashSet<ClassKey>>) -> bool {
        if checked.is_none() {
            *checked = Some(HashSet::new());
        }
        let class_symbol = &symbol_table[class_key]; // former self on method
        for b in class_symbol.bases.iter().filter_map(|w| w.upgrade(symbol_table)) {
            if b == base {
                return true;
            }
            let checked_mut = checked.as_mut().unwrap();
            if checked_mut.contains(&b) {
                return false;
            }
            checked_mut.insert(b);
            if ClassSymbol::inherits(symbol_table, b, base, checked) {
                return true;
            }
        }
        false
    }

    // @arena: moved to SymbolTable
    // pub fn get_ext_symbol(&self, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
    //     let mut result = vec![];
    //     if let Some(owners) = self.ext_symbols.get(name) {
    //         for owner in owners.iter() {
    //             let owner = owner.borrow();
    //             result.extend(owner.get_decl_ext_symbol(&self.weak_self.as_ref().unwrap().upgrade().unwrap(), name));
    //         }
    //     }
    //     result
    // }

    // @arena: moved to SymbolView
    // pub fn get_decl_ext_symbol(&self, symbol: &Rc<RefCell<Symbol>>, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
    //     let mut result = vec![];
    //     if let Some(object_decl_symbols) = self.decl_ext_symbols.get(symbol) {
    //         if let Some(symbols) = object_decl_symbols.get(name) {
    //             for end_symbols in symbols.values() {
    //                 //TODO actually we don't take position into account, but can we really?
    //                 result.extend(end_symbols.iter().map(|s| s.clone()));
    //             }
    //         }
    //     }
    //     result
    // }

}
