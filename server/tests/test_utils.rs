use once_cell::sync::Lazy;
use std::{cell::RefCell, rc::Rc, cmp::Ordering};

use odoo_ls_server::{core::{file_mgr::FileInfo, symbols::symbol::Symbol}, threads::SessionInfo, utils::compare_semver};


/// Returns the correct class name for Partner/ResPartner depending on Odoo version
pub static PARTNER_CLASS_NAME: Lazy<fn(&str) -> &'static str> = Lazy::new(|| {
    |full_version: &str| {
        if compare_semver(full_version, "18.1") >= Ordering::Equal {
            "ResPartner"
        } else {
            "Partner"
        }
    }
});

/// Returns the correct class name for Country/ResCountry depending on Odoo version
pub static COUNTRY_CLASS_NAME: Lazy<fn(&str) -> &'static str> = Lazy::new(|| {
    |full_version: &str| {
        if compare_semver(full_version, "18.1") >= Ordering::Equal {
            "ResCountry"
        } else {
            "Country"
        }
    }
});


/// Helper to get hover markdown string at a given (line, character)
pub fn get_hover_markdown(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<String> {
    let hover = odoo_ls_server::features::hover::HoverFeature::hover_python(
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
pub fn get_definition_locs(session: &mut SessionInfo, f_sym: &Rc<RefCell<Symbol>>, f_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Vec<lsp_types::LocationLink> {
    let locations = odoo_ls_server::features::definition::DefinitionFeature::get_location(
        session,
        f_sym,
        f_info,
        line,
        character,
    );
    let locations = locations.map(|l| {
        match l {
            lsp_types::GotoDefinitionResponse::Link(locs) => locs,
            _ => unreachable!("Expected GotoDefinitionResponse::Link"),
        }
    }).into_iter().flatten().collect::<Vec<_>>();
    locations
}

pub fn diag_on_line(diagnostics: &Vec<lsp_types::Diagnostic>, line: u32) -> Vec<&lsp_types::Diagnostic> {
    diagnostics.iter().filter(|d| d.range.start.line <= line && d.range.end.line >= line).collect()
}