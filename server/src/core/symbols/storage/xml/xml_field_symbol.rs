use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::symbol_keys::SymbolKey};


#[derive(Debug)]
pub struct XmlFieldSymbol {
    pub field_name: OYarn,
    pub range: TextRange,
    pub text: Option<String>,
    pub text_range: Option<TextRange>,
    pub ref_key: Option<(String, TextRange)>,
    pub is_external: bool,

    parent: SymbolKey,
}

impl XmlFieldSymbol {
    pub fn new(
        field_name: OYarn,
        range: TextRange,
        text: Option<String>,
        text_range: Option<TextRange>,
        ref_key: Option<(String, TextRange)>,
        parent: SymbolKey,
        is_external: bool,
    ) -> Self {
        Self {
            field_name,
            range,
            text,
            text_range,
            ref_key,
            parent,
            is_external,
        }
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    /// no child symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }
}