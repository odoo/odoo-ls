use std::path::PathBuf;

use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::diag_on_line};

pub mod ols01000;
pub mod ols01001;
pub mod ols01002;
pub mod ols01003;
pub mod ols01004;
pub mod ols01005;
pub mod ols01006;
pub mod ols01007;
pub mod ols01008;
pub mod ols01009;
pub mod ols01010;
pub mod ols01011;
pub mod ols02001;
pub mod ols02002;
pub mod ols03000_1_to_23;
pub mod ols03018_compute_sql;
pub mod ols03025_to_27;
pub mod ols03011_search_methods;
pub mod ols03028;
pub mod ols03029;
pub mod ols03301;
pub mod ols03302;
pub mod ols04000_1_to_20;
pub mod ols05000s;
pub mod ols05073_74;
pub mod implicit_class_method_no_ols_01007;
pub mod panic_super_arg;

/// Failures of `cases` in `file` of `module_for_diagnostics/models`: each case is a statement as
/// written, found by its line, then the expected code and the text it points at if invalid, as the
/// only Error of that line.
pub(crate) fn case_failures(file: &str, cases: &[(&str, Option<(&str, &str)>)]) -> Vec<String> {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_for_diagnostics")
        .join("models")
        .join(file)
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let lines: Vec<String> = std::fs::read_to_string(&path).unwrap().lines().map(str::to_string).collect();
    let mut failures = vec![];
    for &(statement, expected) in cases {
        let Some(line) = lines.iter().position(|text| text.trim() == statement) else {
            failures.push(format!("{statement}: no such line in {file}"));
            continue;
        };
        let text = &lines[line];
        let found = diag_on_line(&diagnostics, line as u32);
        let as_expected = match expected {
            None => found.is_empty(),
            Some((code, pointed)) => matches!(found.as_slice(), [diagnostic]
                if matches!(&diagnostic.code, Some(NumberOrString::String(c)) if c == code)
                && diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && pointed_text(diagnostic, text) == Some(pointed)),
        };
        if !as_expected {
            let found: Vec<_> = found.iter().map(|diagnostic| describe(diagnostic, text)).collect();
            failures.push(format!("{statement}: expected {expected:?}, got {found:?}"));
        }
    }
    failures
}

/// Text of `line` under a diagnostic on that line only.
fn pointed_text<'a>(diagnostic: &Diagnostic, line: &'a str) -> Option<&'a str> {
    let range = diagnostic.range;
    if range.start.line != range.end.line {
        return None;
    }
    line.get(range.start.character as usize..range.end.character as usize)
}

fn describe(diagnostic: &Diagnostic, line: &str) -> String {
    let code = match &diagnostic.code {
        Some(NumberOrString::String(code)) => code.clone(),
        other => format!("{other:?}"),
    };
    format!("{code} {:?} on {:?}: {}", diagnostic.severity, pointed_text(diagnostic, line), diagnostic.message)
}
