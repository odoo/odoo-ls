use lsp_types::{Diagnostic, NumberOrString};
use once_cell::sync::Lazy;
use std::{cell::RefCell, cmp::Ordering, collections::{HashMap, HashSet}, rc::Rc};

use odoo_ls_server::{S, core::{file_mgr::FileInfo, symbols::symbol::Symbol}, threads::SessionInfo, utils::compare_semver};


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

/**
 * Verify that the given diagnostics match the expected diagnostics from doc_diag, generated from comments in the source code
 */
pub fn verify_diagnostics_against_doc(
    diagnostics: Vec<Diagnostic>,
    doc_diag: Vec<(u32, Vec<String>)>
) {
    // Build a map from line to set of diagnostic codes found in diagnostics
    let mut diags: HashMap<u32, Vec<&Diagnostic>> = HashMap::new();
    for diag in &diagnostics {
        let line = diag.range.start.line;
        let code_str = match &diag.code {
            Some(NumberOrString::String(c)) => c.clone(),
            Some(NumberOrString::Number(n)) => n.to_string(),
            None => continue,
        };
        diags.entry(line).or_default().push(diag);
    }

    // Check expected codes and unexpected codes in a single loop
    for (line, expected_codes) in &doc_diag {
        let found_codes = diags.get(line);
        assert!(found_codes.is_some(), "No diagnostics found on line {}. {} {} expected", line + 1, expected_codes.join(", "), if expected_codes.len() > 1 { "were" } else { "was" });

        let found_codes = found_codes.unwrap();
        // Check that all expected codes are present
        for code in expected_codes {
            assert!(
                found_codes.iter().any(|d| match &d.code {
                    Some(NumberOrString::String(c)) => c == code,
                    Some(NumberOrString::Number(n)) => &n.to_string() == code,
                    None => false,
                }),
                "Expected diagnostic code '{}' on line {}, but not found",
                code,
                line + 1
            );
        }

        // Check that no unexpected codes are present
        for code in found_codes.iter().map(|d| match &d.code {
            Some(NumberOrString::String(c)) => c.clone(),
            Some(NumberOrString::Number(n)) => n.to_string(),
            None => panic!("Diagnostic code is None"),
        }) {
            assert!(
                expected_codes.contains(&code),
                "Unexpected diagnostic code '{}' on line {}",
                code,
                line + 1
            );
        }
    }

    // Also check for diagnostics on lines not in doc_diag
    let expected_lines: HashSet<u32> = doc_diag.iter().map(|(l, _)| *l).collect();
    for line in diags.keys() {
        assert!(
            expected_lines.contains(line),
            "Unexpected diagnostics on line {}: {}",
            line + 1,
            diags.get(line).unwrap().iter().map(|d| match &d.code {
                Some(NumberOrString::String(c)) => S!("(") + c.as_str() + ") - " + d.message.as_str(),
                Some(NumberOrString::Number(n)) => S!("(") + n.to_string().as_str() + ") - " + d.message.as_str(),
                None => "None".to_string(),
            }).collect::<Vec<String>>().join(", "),
        );
    }
}
