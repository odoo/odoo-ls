use lsp_types::request::GotoDeclarationResponse;
use lsp_types::{LocationLink, Range};
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

use crate::constants::SymType;
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::core::symbols::symbol::Symbol;
use crate::features::goto_utils::{GotoRequest, GotoUtils};
use crate::features::xml_ast_utils::{XmlAstResult, XmlAstUtils};
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;

pub struct DeclarationFeature {}

impl DeclarationFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDeclarationResponse> {
        let definitions_sources = GotoUtils::get_symbols(session, GotoRequest::Declaration, file_symbol, file_info, line, character);
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        Some(GotoDeclarationResponse::Link(links))
    }

    pub fn get_location_xml(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDeclarationResponse> {
        let definitions_sources = GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character);
        let mut links = vec![];
        for xml_result in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, xml_result));
        }
        Some(GotoDeclarationResponse::Link(links))
    }

    pub fn get_location_csv(_session: &mut SessionInfo,
        _file_symbol: &Rc<RefCell<Symbol>>,
        _file_info: &Rc<RefCell<FileInfo>>,
        _line: u32,
        _character: u32
    ) -> Option<GotoDeclarationResponse> {
        None
    }

}
