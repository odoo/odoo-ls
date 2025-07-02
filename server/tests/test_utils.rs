use std::{cell::RefCell, rc::Rc};

use odoo_ls_server::{core::{file_mgr::FileInfo, symbols::symbol::Symbol}, threads::SessionInfo};



/// Helper to get hover markdown string at a given (line, character)
pub fn get_hover_markdown(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<String> {
    let hover = odoo_ls_server::features::hover::HoverFeature::get_hover(
        session,
        file_symbol,
        file_info,
        line,
        character,
    );
    hover.and_then(|h| match h.contents {
        lsp_types::HoverContents::Markup(m) => Some(m.value),
        lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(s)) => Some(s),
        _ => None,
    })
}

/// Helper to get hover markdown string at a given (line, character)
pub fn get_definition_locs(session: &mut SessionInfo, f_sym: &Rc<RefCell<Symbol>>, f_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Vec<lsp_types::Location> {
    let locations = odoo_ls_server::features::definition::DefinitionFeature::get_location(
        session,
        f_sym,
        f_info,
        line,
        character,
    );
    let locations = locations.map(|l| {
        match l {
            lsp_types::GotoDefinitionResponse::Array(locs) => locs,
            _ => unreachable!("Expected GotoDefinitionResponse::Array"),
        }
    }).into_iter().flatten().collect::<Vec<_>>();
    locations
}