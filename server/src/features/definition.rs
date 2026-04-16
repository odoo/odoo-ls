use lsp_types::{GotoDefinitionResponse};
use std::{cell::RefCell, rc::Rc};

use crate::core::file_mgr::{AstType, FileInfo};
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::goto_utils::{GotoRequest, GotoUtils};
use crate::threads::SessionInfo;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
        let definitions_sources = match ast_type {
            AstType::Python => GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character),
            AstType::Xml => GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character),
            AstType::Csv => GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character),
        };
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        Some(GotoDefinitionResponse::Link(links))
    }
}
