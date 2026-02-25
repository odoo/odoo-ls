use std::env;

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::{S, utils::PathSanitizer};

use crate::{setup::setup::*, test_utils::diag_on_line};

#[test]
fn test_ols010_04_and_11() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/diagnostics/ols010_04_and_11.py").sanitize();
    prepare_custom_entry_point(&mut session, &path);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    assert_eq!(diagnostics.len(), 2);

    // OLS01011
    let line_diagnostics = diag_on_line(&diagnostics, 30);
    assert_eq!(line_diagnostics.len(), 1);
    let diag = &line_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS01011"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS01004
    let line_diagnostics = diag_on_line(&diagnostics, 33);
    assert_eq!(line_diagnostics.len(), 1);
    let diag = &line_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS01004"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));
}