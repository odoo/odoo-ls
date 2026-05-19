use crate::utils::HashMap;

use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::symbol_keys::{SymbolKey, XmlFieldKey}};

#[derive(Debug)]
pub struct XmlAssetSymbol {
    pub xml_id: Option<OYarn>,
    pub is_external: bool,
    pub (in crate::core::symbols::storage) fields: HashMap<OYarn, XmlFieldKey>,
    pub range: TextRange,

    parent: SymbolKey,
}

impl XmlAssetSymbol {
    pub fn new(xml_id: Option<OYarn>, range: TextRange, parent: SymbolKey, is_external: bool) -> Self {
        Self { xml_id, range, parent, is_external, fields: HashMap::default() }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        self.fields.values().map(|k| SymbolKey::XmlField(*k)).collect()
    }
}