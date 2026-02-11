use ruff_text_size::{TextRange, TextSize};
use std::collections::{HashMap, HashSet};
use std::cell::RefCell;

use crate::constants::OYarn;
use crate::core::file_mgr::NoqaInfo;
use crate::core::model::ModelData;
use crate::oyarn;
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey, Weak};
use crate::threads::SessionInfo;
use crate::utils::NoHashBuilder;

use super::symbol_mgr::{SectionRange, SymbolMgr};


#[derive(Debug)]
pub struct ClassSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub bases: Vec<Weak<ClassKey>>, // formely Vec<Weak<RefCell<Symbol>>>
    parent: SymbolKey,
    pub range: TextRange,
    pub body_range: TextRange,
    pub _model: Option<ModelData>,
    pub noqas: NoqaInfo,
    pub(crate) _is_field_class: RefCell<Option<bool>>, //cache, do not call directly, use is_field_class() method instead

    //Trait SymbolMgr
    //--- Body symbols
    pub sections: Vec<SectionRange>,
    pub(super) symbols: HashMap<OYarn, HashMap<u32, Vec<SymbolKey>, NoHashBuilder>>,
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

    pub fn inherits(session: &mut SessionInfo, class_key: ClassKey, base: ClassKey, checked: &mut Option<HashSet<ClassKey>>) -> bool {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        if checked.is_none() {
            *checked = Some(HashSet::new());
        }
        let bases: Vec<_> = st!()[class_key].bases.iter().filter_map(|w| w.upgrade(&st!())).collect();
        for b in bases {
            if b == base {
                return true;
            }
            let checked_mut = checked.as_mut().unwrap();
            if checked_mut.contains(&b) {
                continue;
            }
            checked_mut.insert(b);
            if ClassSymbol::inherits(session, b, base, checked) {
                return true;
            }
        }
        if let (Some(self_model), Some(base_model)) = (
            st!()[class_key]._model.as_ref().and_then(|model_data|
                session.sync_odoo.models.get(&model_data.name).cloned()
            ),
            st!()[base]._model.as_ref().and_then(|model_data|
                session.sync_odoo.models.get(&model_data.name).cloned()
            )){
            if self_model.borrow().inherits_from(session, &base_model) {
                return true;
            }
        }
        false
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
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
