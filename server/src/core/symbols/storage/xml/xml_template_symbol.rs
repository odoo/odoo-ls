use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::{symbols::symbol_keys::SymbolKey}};

#[derive(Debug)]
pub struct XmlTemplateSymbol {
    pub xml_id: Option<OYarn>,
    pub t_name: Option<OYarn>,
    pub is_web: bool,
    pub is_external: bool,
    pub range: TextRange,
    /// (template_name, attribute_range) for each t-call found in this template body
    pub t_calls: Vec<(OYarn, TextRange)>,

    parent: SymbolKey,
}

impl XmlTemplateSymbol {
    pub fn new(xml_id: Option<OYarn>, t_name: Option<OYarn>, range: TextRange, parent: SymbolKey, is_web: bool, is_external: bool) -> Self {
        Self { xml_id, t_name, range, t_calls: vec![], parent, is_web, is_external }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    /// no child symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }
}