use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::{symbols::symbol_keys::SymbolKey}};

#[derive(Debug)]
pub struct XmlDeleteSymbol {
    pub xml_id: Option<OYarn>,
    pub is_external: bool,
    pub range: TextRange,
    pub model: OYarn,

    parent: SymbolKey,
}

impl XmlDeleteSymbol {
    pub fn new(xml_id: Option<OYarn>, range: TextRange, model: OYarn, parent: SymbolKey, is_external: bool) -> Self {
        Self { xml_id, range, parent, is_external, model }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    /// no child symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }
}