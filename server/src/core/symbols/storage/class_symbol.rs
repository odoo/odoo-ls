use ruff_text_size::{TextRange, TextSize};
use std::collections::{HashMap, HashSet};
use std::cell::RefCell;

use crate::constants::OYarn;
use crate::core::file_mgr::NoqaInfo;
use crate::core::model::ModelData;
use crate::oyarn;
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey, Wk};
use crate::threads::SessionInfo;
use crate::utils::NoHashBuilder;

use super::symbol_mgr::{SectionRange, SymbolMgr};


#[derive(Debug)]
pub struct ClassSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub bases: Vec<Wk<ClassKey>>,
    pub range: TextRange,
    pub body_range: TextRange,
    pub _model: Option<ModelData>,
    pub noqas: NoqaInfo,
    pub(crate) _is_field_class: RefCell<Option<bool>>, //cache, do not call directly, use is_field_class() method instead

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,

    // parent / child symbols
    parent: SymbolKey,
    //--- Body symbols
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
        if checked.is_none() {
            *checked = Some(HashSet::new());
        }
        let bases: Vec<_> = session.st()[class_key].bases.iter().filter_map(|w| w.upgrade(session.st())).collect();
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
            session.st()[class_key]._model.as_ref().and_then(|model_data|
                session.sync_odoo.models.get(&model_data.name).cloned()
            ),
            session.st()[base]._model.as_ref().and_then(|model_data|
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

    pub fn children(&self) -> Vec<SymbolKey> {
        self.symbols.values()
            .flat_map(|section| section.values())
            .flatten()
            .copied().collect()
    }

}
