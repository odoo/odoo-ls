use crate::setup::shared::with_shared_session;
use std::env;

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::{S, utils::PathSanitizer};

use crate::setup::setup::*;

#[test]
fn test_ols01000() {
    with_shared_session(false, |session| {
        let path = env::current_dir().unwrap().join("tests/data/python/diagnostics/ols01000.py").sanitize();
        prepare_custom_entry_point(session, &path);
        let diagnostics = get_diagnostics_for_path(session, &path);
        assert_eq!(diagnostics.len(), 1);
        let diag = &diagnostics[0];
        assert!(diag.code.is_some());
        let code = match &diag.code {
            Some(NumberOrString::String(code)) => code,
            Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
            None => panic!("Diagnostic code is None"),
        };
        assert!(code == &S!("OLS01000"));
        assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));
    });
}