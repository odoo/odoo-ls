use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::storage::XmlDataParent};

#[derive(Debug)]
pub struct XmlTemplateSymbol {
    pub xml_id: Option<OYarn>,
    /// (template_name, value_range) of `t-name`, if any (quotes excluded).
    pub t_name: Option<(OYarn, TextRange)>,
    pub is_web: bool,
    pub is_external: bool,
    pub range: TextRange,
    /// (template_name, value_range) for each t-call found in this template body (quotes exclued)
    pub t_calls: Vec<(OYarn, TextRange)>,
    /// (template_name, value_range) of the `t-inherit` this template extends, if any (quotes excluded).
    /// A template element carries at most one `t-inherit`.
    pub t_inherit: Option<(OYarn, TextRange)>,

    parent: XmlDataParent,
}

impl XmlTemplateSymbol {
    pub fn new(xml_id: Option<OYarn>, t_name: Option<(OYarn, TextRange)>, range: TextRange, parent: XmlDataParent, is_web: bool, is_external: bool) -> Self {
        Self { xml_id, t_name, range, t_calls: vec![], t_inherit: None, parent, is_web, is_external }
    }

    pub fn parent(&self) -> XmlDataParent {
        self.parent
    }
}
