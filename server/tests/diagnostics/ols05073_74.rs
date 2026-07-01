use std::path::PathBuf;

use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};
use odoo_ls_server::{S, utils::PathSanitizer};

use crate::{setup::setup::*, test_utils::diag_on_line};

fn check_xml_diagnostic(diagnostics: &[Diagnostic], ols_code: &str, line: u32, severity: DiagnosticSeverity) {
    let line_diagnostics = diag_on_line(diagnostics, line);
    assert_eq!(
        line_diagnostics.len(), 1,
        "Expected exactly 1 diagnostic at line {line}, found {}: {:?}",
        line_diagnostics.len(),
        line_diagnostics
    );
    let diag = &line_diagnostics[0];
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {num}"),
        None => panic!("Diagnostic code is None"),
    };
    assert_eq!(code, &S!(ols_code), "Expected code {ols_code} at line {line}, got {code}");
    assert!(diag.severity.is_some_and(|s| s == severity), "Expected severity {severity:?} for {ols_code} at line {line}");
}

/// OLS05073: t-call to a template that does not exist (backend XML template)
/// OLS05074: t-call to a template whose module is not declared as a dependency (backend)
#[test]
fn test_ols05073_05074_backend_template() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let xml_path = test_addons_path
        .join("module_templates_b")
        .join("data")
        .join("backend_templates.xml");
    assert!(xml_path.exists(), "Test file does not exist: {}", xml_path.display());
    let diagnostics = get_diagnostics_for_path(&mut session, &xml_path.sanitize());
    // Line 2: <t t-call="module_templates_b.nonexistent_template"/> → not found
    check_xml_diagnostic(&diagnostics, "OLS05073", 2, DiagnosticSeverity::ERROR);
    // Line 5: <t t-call="module_templates_a.backend_template_a"/> → exists but module not in deps
    check_xml_diagnostic(&diagnostics, "OLS05074", 5, DiagnosticSeverity::ERROR);
}

/// OLS05074: t-call to a frontend template whose module is not declared as a dependency
#[test]
fn test_ols05074_frontend_template() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let xml_path = test_addons_path
        .join("module_templates_b")
        .join("static")
        .join("src")
        .join("frontend_templates.xml");
    assert!(xml_path.exists(), "Test file does not exist: {}", xml_path.display());
    let diagnostics = get_diagnostics_for_path(&mut session, &xml_path.sanitize());
    // Line 2: <t t-call="module_templates_a.FrontendTemplate"/> → exists but module not in deps
    check_xml_diagnostic(&diagnostics, "OLS05074", 2, DiagnosticSeverity::ERROR);
}
