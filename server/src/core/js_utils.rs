use oxc::diagnostics::OxcDiagnostic;

use crate::{S, constants::EXTENSION_NAME};

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use regex::Regex;

pub fn oxc_diagnostic_to_lsp_diagnostic(diag: &OxcDiagnostic, uri: &lsp_types::Uri) -> Option<lsp_types::Diagnostic> {
    let labels = diag.labels.as_ref()?;
    let first_label = labels.first().unwrap();
    let range = lsp_types::Range {
        start: lsp_types::Position { line: first_label.offset() as u32, character: 0 },
        end: lsp_types::Position { line: first_label.offset() as u32 + first_label.len() as u32, character: 0 },
    };
    let related_information = labels.iter().skip(1)
        .filter_map(|label| {
            let msg = label.label()?.to_string();
            let start = label.offset();
            let end   = start + label.len();
            Some(lsp_types::DiagnosticRelatedInformation {
                location: lsp_types::Location {
                    uri: uri.clone(),
                    range: lsp_types::Range {
                        start: lsp_types::Position { line: start as u32, character: 0 },
                        end:   lsp_types::Position { line: end   as u32, character: 0 },
                    },
                },
                message: msg,
            })
        })
        .collect::<Vec<_>>();
    let message = match first_label.label() {
        Some(label) => format!("{} - {}", diag.message.clone(), label),
        None => diag.message.clone().to_string(),
    };
    Some(lsp_types::Diagnostic {
        range,
        severity: Some(match diag.severity {
            oxc::diagnostics::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            oxc::diagnostics::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            oxc::diagnostics::Severity::Advice => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        code: Some(lsp_types::NumberOrString::String(format!("oxc_{}", diag.code))),
        code_description: None,
        source: Some(S!(EXTENSION_NAME)),
        message,
        related_information: if related_information.is_empty() { None } else { Some(related_information) },
        tags: None,
        data: None,
    })
}

/// Ported from `ODOO_MODULE_RE` (`odoo/tools/js_transpiler.py`) 
/// For parsing "// @odoo-module" headers in js files
static ODOO_MODULE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        ^\s*                           # starting white space
        /[*/]                          # /* or //
        .*                             # any comment in between (optional)
        @odoo-module                   # '@odoo-module' statement
        (?<ignore>\s+ignore)?          # opt-out (optional)
        (\s+alias=(?<alias>[^\s*]+))?  # alias (optional)
        (\s+default=(?<default>[\w$]+))?   # no implicit default export (optional)
        ",
    )
    .unwrap()
});

/// A JS file's `@odoo-module` header
#[derive(Debug, PartialEq)]
pub struct ModuleHeader {
    /// `ignore`: the file opts out (not importable)
    pub ignore: bool,
    /// `alias=X`: a *second* `odoo.define` name, added to the file's own path name. Both work.
    pub alias: Option<String>,
    // `default`: we don't use it
}

// More than enough to read a "@odoo-module" header in the first line
// We don't want to read a whole single-line minified lib 
const BUFFER_SIZE: usize = 256;

/// The file's "@odoo-module" header, or `None` when it has none
pub fn read_module_header(file: &Path) -> Option<ModuleHeader> {
    let mut buffer = [0u8; BUFFER_SIZE];
    let read = std::fs::File::open(file).ok()?.read(&mut buffer).ok()?;
    parse_module_header(&String::from_utf8_lossy(&buffer[..read]))
}

/// Only the first line is read, so a whole source may be handed over as well as a head.
fn parse_module_header(head: &str) -> Option<ModuleHeader> {
    let captures = ODOO_MODULE_RE.captures(head)?;
    Some(ModuleHeader {
        ignore: captures.name("ignore").is_some(),
        alias: captures.name("alias").map(|alias| alias.as_str().to_string()),
    })
}


#[cfg(test)]
mod tests {
    use crate::S;

    use super::*;

    fn alias_of(head: &str) -> Option<Option<String>> {
        parse_module_header(head).map(|header| header.alias)
    }

    #[test]
    fn header_decides_whether_a_lib_file_is_importable() {
        // Verbatim headers from `web/static/lib`.
        assert_eq!(alias_of("/** @odoo-module */\nexport const a = 1;"), Some(None));
        assert_eq!(
            alias_of("/** @odoo-module alias=@odoo/hoot-dom default=false */"),
            Some(Some(S!("@odoo/hoot-dom")))
        );
        // `owl.js`: a plain bundle, so nothing may be imported from it.
        assert_eq!(alias_of("\"use strict\";\nvar owl = (() => {"), None);
        // `ODOO_MODULE_RE` ends the alias at a `*`, spaced or not.
        assert_eq!(alias_of("/** @odoo-module alias=@odoo/hoot*/"), Some(Some(S!("@odoo/hoot"))));
        // Opting out beats everything else on the line.
        assert_eq!(
            parse_module_header("// @odoo-module ignore alias=@web/nope"),
            Some(ModuleHeader { ignore: true, alias: Some(S!("@web/nope")) })
        );
    }
}
