use lsp_types::{GotoDefinitionResponse};
use std::{cell::RefCell, rc::Rc};

use crate::core::file_mgr::{AstType, FileInfo};
use crate::core::symbols::symbol::Symbol;
use crate::features::goto_utils::{GotoRequest, GotoUtils};
use crate::threads::SessionInfo;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
        let function = match ast_type {
            AstType::Python => DefinitionFeature::_get_location_py,
            AstType::Xml => DefinitionFeature::_get_location_xml,
            AstType::Csv => DefinitionFeature::_get_location_csv,
        };
        function(session, file_symbol, file_info, line, character)
    }

    pub fn _get_location_py(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let definitions_sources = GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character);
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        Some(GotoDefinitionResponse::Link(links))
    }

    pub fn _get_location_xml(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let definitions_sources = GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character);
        let mut links = vec![];
        for xml_result in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, xml_result));
        }
        Some(GotoDefinitionResponse::Link(links))
    }

    pub fn _get_location_csv(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let definitions_sources = GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character);
        let mut links = vec![];
        for xml_result in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, xml_result));
        }
        Some(GotoDefinitionResponse::Link(links))
    }

}
