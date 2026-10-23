use oxc::diagnostics::{LabeledSpan, OxcDiagnostic};
use ruff_text_size::TextRange;

use crate::core::diagnostics::Diag;
use crate::utils::range_to_text_range;

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use regex::Regex;

pub fn oxc_diagnostic_to_diag(diag: &OxcDiagnostic) -> Option<Diag> {
    let labels = diag.labels.as_ref()?;
    let first_label = labels.first()?;
    // LabeledSpan uses byte offsets
    let range_of_label = |label: &LabeledSpan| -> TextRange {
        let start = label.offset();
        let end = start + label.len();
        range_to_text_range(start..end)
    };
    let range = range_of_label(first_label);
    let message = match first_label.label() {
        Some(label) => format!("{} - {}", diag.message, label),
        None => diag.message.to_string(),
    };
    let code = if diag.code.is_some() {
        format!("oxc_{}", diag.code)
    } else {
        "oxc_syntax".to_string()
    };
    let severity = match diag.severity {
        oxc::diagnostics::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
        oxc::diagnostics::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
        oxc::diagnostics::Severity::Advice => lsp_types::DiagnosticSeverity::INFORMATION,
    };
    let mut diagnostic = Diag::external(code, message, range, severity);
    for label in labels.iter().skip(1) {
        let Some(label_msg) = label.label() else { continue };
        let range = range_of_label(label);
        diagnostic.add_related_info(label_msg.to_string(), range);
    }
    Some(diagnostic)
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

/// Whether a file has the "@odoo-module header", without "ignore".
/// Useful for defining whether a file in `static/lib` is one of Odoo's JS modules.
pub fn is_headed_odoo_module(contents: &str) -> bool {
    parse_module_header(contents).is_some_and(|header| !header.ignore)
}

#[cfg(test)]
mod tests {
    use crate::S;
    use oxc::span::Span;

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

    #[test]
    fn is_odoo_module_reads_whole_sources() {
        assert!(is_headed_odoo_module("/** @odoo-module */\nexport const a = 1;\n"));
        assert!(!is_headed_odoo_module("\"use strict\";\nvar owl = (() => {\n"));
        assert!(!is_headed_odoo_module("// @odoo-module ignore\nexport const a = 1;\n"));
        // A header may not start on a later line: `ODOO_MODULE_RE` is anchored.
        assert!(!is_headed_odoo_module("const a = 1;\n/** @odoo-module */\n"));
    }

    fn code_of(diag: OxcDiagnostic) -> String {
        oxc_diagnostic_to_diag(&diag).unwrap().code().to_string()
    }

    #[test]
    fn oxc_diagnostics_always_carry_a_prefixed_code() {
        // Parser diags have an empty `OxcCode`, which would render as the bare `oxc_`.
        // So we call them "oxc_syntax"
        let syntax = OxcDiagnostic::error("Unexpected token").with_label(Span::new(0, 3));
        assert_eq!(code_of(syntax), "oxc_syntax");

        // Lint diags keep `scope(number)`.
        let lint = OxcDiagnostic::warn("`a` is never reassigned")
            .with_error_code("eslint", "no-const-assign")
            .with_label(Span::new(0, 3));
        assert_eq!(code_of(lint), "oxc_eslint(no-const-assign)");
    }
}
