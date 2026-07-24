use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::storage::XmlDataParent};

#[derive(Debug)]
pub struct XmlDeleteSymbol {
    pub xml_id: Option<OYarn>,
    pub is_external: bool,
    pub range: TextRange,
    pub model: OYarn,

    parent: XmlDataParent,
}

impl XmlDeleteSymbol {
    pub fn new(xml_id: Option<OYarn>, range: TextRange, model: OYarn, parent: XmlDataParent, is_external: bool) -> Self {
        Self { xml_id, range, parent, is_external, model }
    }

    pub fn parent(&self) -> XmlDataParent {
        self.parent
    }
}
