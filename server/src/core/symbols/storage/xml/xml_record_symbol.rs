use std::{ops::Range};
use crate::utils::HashMap;

use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::symbol_keys::{SymbolKey, XmlFieldKey}};

#[derive(Debug)]
pub struct XmlRecordSymbol {
    pub is_external: bool,
    pub model: (OYarn, Range<usize>),
    pub xml_id: Option<OYarn>,
    pub (in crate::core::symbols::storage) fields: HashMap<OYarn, XmlFieldKey>,
    pub range: TextRange,

    parent: SymbolKey,
}

impl XmlRecordSymbol {
    pub fn new(
        model: (OYarn, Range<usize>),
        xml_id: Option<OYarn>,
        range: TextRange,
        parent: SymbolKey,
        is_external: bool,
    ) -> Self {
        Self {
            model,
            xml_id,
            fields: HashMap::default(),
            range,
            parent,
            is_external,
        }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        self.fields.values().map(|k| SymbolKey::XmlField(*k)).collect()
    }

    pub fn fields(&self) -> &HashMap<OYarn, XmlFieldKey> {
        &self.fields
    }
}