use odoo_ls_server::core::{file_mgr::FileInfoKey, symbols::symbol_keys::SourceFileKey};
use odoo_ls_server::threads::SessionInfo;

type HoverCallable = fn(
    &mut SessionInfo,
    SourceFileKey,
    FileInfoKey,
    u32,
    u32,
) -> Option<lsp_types::Hover>;

fn get_hover(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: FileInfoKey,
    line: u32,
    character: u32,
    callable: HoverCallable,
) -> Option<String> {
    let hover = callable(session, file_symbol, file_info, line, character);
    hover.and_then(|h| match h.contents {
        lsp_types::HoverContents::Markup(m) => Some(m.value),
        lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(s)) => Some(s),
        _ => None,
    })
}

/// Helper to get hover markdown string at a given (line, character)
pub fn get_hover_markdown(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: FileInfoKey,
    line: u32,
    character: u32,
) -> Option<String> {
    get_hover(
        session,
        file_symbol,
        file_info,
        line,
        character,
        odoo_ls_server::features::hover::HoverFeature::hover_python,
    )
}

/// Helper to get XML hover markdown string at a given (line, character)
pub fn get_hover_xml_markdown(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: FileInfoKey,
    line: u32,
    character: u32,
) -> Option<String> {
    get_hover(
        session,
        file_symbol,
        file_info,
        line,
        character,
        odoo_ls_server::features::hover::HoverFeature::hover_xml,
    )
}

/// Helper to get hover markdown string at a given (line, character)
pub fn get_definition_locs(
    session: &mut SessionInfo,
    f_sym: SourceFileKey,
    f_info: FileInfoKey,
    line: u32,
    character: u32,
) -> Vec<lsp_types::LocationLink> {
    let locations = odoo_ls_server::features::definition::DefinitionFeature::get_location(session, f_sym, f_info, line, character);
    locations
        .map(|l| match l {
            lsp_types::GotoDefinitionResponse::Link(locs) => locs,
            _ => unreachable!("Expected GotoDefinitionResponse::Link"),
        })
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
}
