use lsp_types::{SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensLegend};
use crate::core::file_mgr::FileInfo;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::threads::SessionInfo;
use std::rc::Rc;
use std::cell::RefCell;

pub struct SemanticTokensFeature {}

impl SemanticTokensFeature {

    /// Single source of truth for the token legend advertised to the client.
    pub fn legend() -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: vec![
                SemanticTokenType::NAMESPACE,
                SemanticTokenType::CLASS,
                SemanticTokenType::FUNCTION,
                SemanticTokenType::METHOD,
                SemanticTokenType::PARAMETER,
                SemanticTokenType::VARIABLE,
                SemanticTokenType::PROPERTY,
                SemanticTokenType::DECORATOR,
                SemanticTokenType::KEYWORD,
            ],
            token_modifiers: vec![
                SemanticTokenModifier::DECLARATION,
                SemanticTokenModifier::DEFINITION,
                SemanticTokenModifier::READONLY,
                SemanticTokenModifier::STATIC,
                SemanticTokenModifier::DEFAULT_LIBRARY,
            ],
        }
    }

    pub fn tokens_python(_session: &mut SessionInfo, _file_symbol: SourceFileKey, _file_info: &Rc<RefCell<FileInfo>>) -> SemanticTokens {
        // TODO: implement token classification and AST walking
        SemanticTokens { result_id: None, data: vec![] }
    }
}
