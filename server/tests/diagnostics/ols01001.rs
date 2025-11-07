use std::env;

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::{S, utils::PathSanitizer};

use crate::setup::setup::*;

#[test]
fn test_ols01001() {
    let mut odoo = setup_server(false);
    let path = env::current_dir().unwrap().join("tests/data/python/diagnostics/ols01001.py").sanitize();
    let mut session = prepare_custom_entry_point(&mut odoo, &path);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    assert_eq!(diagnostics.len(), 1);
    let diag = &diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS01001"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::WARNING));
}