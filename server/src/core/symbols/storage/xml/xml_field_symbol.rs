use ruff_text_size::TextRange;

use crate::{constants::OYarn, core::symbols::symbol_keys::SymbolKey};
use std::fmt::Display;


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

/// Enum for the possible field names of an xml field symbol, used to get the text of these fields in a type safe way
// Add a new enum values as needed
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum XmlFieldName {
    Name,
    Type,
    Relation,
    Model,
    ModelId,
    Code,
    Id,
}

impl XmlFieldName {
    pub fn as_str(&self) -> &'static str {
        match self {
            XmlFieldName::Name => "name",
            XmlFieldName::Type => "ttype",
            XmlFieldName::Relation => "relation",
            XmlFieldName::Model => "model",
            XmlFieldName::ModelId => "model_id",
            XmlFieldName::Code => "code",
            XmlFieldName::Id => "id",
        }
    }
}

impl Display for XmlFieldName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
