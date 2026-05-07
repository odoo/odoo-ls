use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::{symbols::symbol_keys::SymbolKey}};

#[derive(Debug)]
pub struct XmlTemplateSymbol {
    pub xml_id: Option<OYarn>,
    pub is_external: bool,
    pub range: TextRange,

    parent: SymbolKey,
}

impl XmlTemplateSymbol {
    pub fn new(xml_id: Option<OYarn>, range: TextRange, parent: SymbolKey, is_external: bool) -> Self {
        Self { xml_id, range, parent, is_external }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    /// no child symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }
}