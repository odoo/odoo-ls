use crate::{core::symbols::storage::XmlDataParent, utils::HashMap};

use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::symbol_keys::XmlFieldKey};

#[derive(Debug)]
pub struct XmlAssetSymbol {
    pub xml_id: Option<OYarn>,
    pub is_external: bool,
    pub range: TextRange,

    parent: XmlDataParent,
    pub(in crate::core::symbols::storage) fields: HashMap<OYarn, XmlFieldKey>,
}

impl XmlAssetSymbol {
    pub fn new(xml_id: Option<OYarn>, range: TextRange, parent: XmlDataParent, is_external: bool) -> Self {
        Self { xml_id, range, parent, is_external, fields: HashMap::default() }
    }

    pub fn parent(&self) -> XmlDataParent {
        self.parent
    }
}
