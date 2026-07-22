use lsp_types::Location;
use lsp_types::request::GotoDeclarationResponse;
use std::{cell::RefCell, rc::Rc};

use crate::core::file_mgr::{Ast, FileInfo};
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::core::tsserver_bridge;
use crate::features::goto_utils::{GotoRequest, GotoUtils};
use crate::threads::SessionInfo;

pub struct DeclarationFeature {}

impl DeclarationFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDeclarationResponse> {
        let ast_type = file_info.borrow().file_info_ast.borrow().ast.clone();
        let definitions_sources = match ast_type {
            Ast::PythonAst(_) => GotoUtils::get_symbols(session, GotoRequest::Declaration, file_symbol, file_info, line, character),
            Ast::XmlAst => GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character),
            Ast::CsvAst => GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character),
            Ast::JsAst(_) => {return Self::get_js_declaration(session, file_info, line, character);},
        };
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        Some(GotoDeclarationResponse::Link(links))
    }

    //Calling get_definition as declaration doesn't have a meaning in javascript
    fn get_js_declaration(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<GotoDeclarationResponse> {
        let file_path = &file_info.borrow().uri;
        let locs: Vec<Location> = if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
            //declaration is not available in javascript, so let's call definition if this route is called for js files.
            bridge.get_definition(file_path, line, character)
                .iter()
                .map(tsserver_bridge::ts_to_lsp_location)
                .collect()
        } else {
            vec![]
        };
        if locs.is_empty() {
            return None;
        }
        Some(GotoDeclarationResponse::Array(locs))
    }
}
