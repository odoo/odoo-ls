use std::path::PathBuf;

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::{S, utils::PathSanitizer};

use crate::{setup::setup::*, test_utils::{diag_on_line, verify_diagnostics_against_doc}};

#[test]
fn test_ols05000_2_3_py_file() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let bikes_py_path = test_addons_path.join("module_for_diagnostics").join("models").join("bike_parts_wheel.py");
    assert!(PathBuf::from(&bikes_py_path).exists(), "Test file does not exist: {}", bikes_py_path.display());
    let bikes_py_diagnostics = get_diagnostics_for_path(&mut session, &bikes_py_path.sanitize());
    let doc_diags = get_diagnostics_test_comments(&mut session, &bikes_py_path.sanitize());
    verify_diagnostics_against_doc(bikes_py_diagnostics, doc_diags); // OLS05002 & OLS05003
}
#[test]
fn test_ols050000_to50012_xml_file() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let bikes_xml_path = test_addons_path.join("module_for_diagnostics").join("data").join("bikes.xml");
    assert!(PathBuf::from(&bikes_xml_path).exists(), "Test file does not exist: {}", bikes_xml_path.display());
    let bikes_xml_diagnostics = get_diagnostics_for_path(&mut session, &bikes_xml_path.sanitize());
    // OLS05001 - Disabled TODO: Re-enable when OLS05001 is implemented
    // OLS05003
    let ols50003_diagnostics = diag_on_line(&bikes_xml_diagnostics, 25);
    assert_eq!(ols50003_diagnostics.len(), 1);
    let diag = &ols50003_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05003"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05004
    let ols50004_diagnostics = diag_on_line(&bikes_xml_diagnostics, 36);
    assert_eq!(ols50004_diagnostics.len(), 1);
    let diag = &ols50004_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05004"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05005
    let ols50005_diagnostics = diag_on_line(&bikes_xml_diagnostics, 38);
    assert_eq!(ols50005_diagnostics.len(), 1);
    let diag = &ols50005_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05005"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05006
    let ols50006_diagnostics = diag_on_line(&bikes_xml_diagnostics, 39);
    assert_eq!(ols50006_diagnostics.len(), 1);
    let diag = &ols50006_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05006"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05007
    let ols50007_diagnostics = diag_on_line(&bikes_xml_diagnostics, 40);
    assert_eq!(ols50007_diagnostics.len(), 1);
    let diag = &ols50007_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05007"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05008
    let ols50008_diagnostics = diag_on_line(&bikes_xml_diagnostics, 41);
    assert_eq!(ols50008_diagnostics.len(), 1);
    let diag = &ols50008_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05008"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05009
    let ols50009_diagnostics = diag_on_line(&bikes_xml_diagnostics, 43);
    assert_eq!(ols50009_diagnostics.len(), 1);
    let diag = &ols50009_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05009"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05010
    let ols50010_diagnostics = diag_on_line(&bikes_xml_diagnostics, 42);
    assert_eq!(ols50010_diagnostics.len(), 1);
    let diag = &ols50010_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05010"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05011
    let ols50011_diagnostics = diag_on_line(&bikes_xml_diagnostics, 45);
    assert_eq!(ols50011_diagnostics.len(), 1);
    let diag = &ols50011_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05011"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));

    // OLS05012
    let ols50012_diagnostics = diag_on_line(&bikes_xml_diagnostics, 48);
    assert_eq!(ols50012_diagnostics.len(), 1);
    let diag = &ols50012_diagnostics[0];
    assert!(diag.code.is_some());
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
        None => panic!("Diagnostic code is None"),
    };
    assert!(code == &S!("OLS05012"));
    assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));
}