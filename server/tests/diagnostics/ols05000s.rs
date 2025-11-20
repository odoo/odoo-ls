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
    verify_diagnostics_against_doc(bikes_py_diagnostics, doc_diags); // OLS05002 & OLS05003 & OLS05051
}

#[test]
fn test_ols05000s_xml_file() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let bikes_xml_path = test_addons_path.join("module_for_diagnostics").join("data").join("bikes.xml");
    assert!(PathBuf::from(&bikes_xml_path).exists(), "Test file does not exist: {}", bikes_xml_path.display());
    let bikes_xml_diagnostics = get_diagnostics_for_path(&mut session, &bikes_xml_path.sanitize());
    let check_xml_diag = |ols_code: &str, line: u32|{
        let line_diagnostics = diag_on_line(&bikes_xml_diagnostics, line);
        assert_eq!(line_diagnostics.len(), 1);
        let diag = &line_diagnostics[0];
        assert!(diag.code.is_some());
        let code = match &diag.code {
            Some(NumberOrString::String(code)) => code,
            Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {}", num),
            None => panic!("Diagnostic code is None"),
        };
        assert!(code == &S!(ols_code));
        assert!(diag.severity.is_some_and(|s| s == DiagnosticSeverity::ERROR));
    };
    // OLS05001 - Disabled TODO: Re-enable when OLS05001 is implemented
    check_xml_diag("OLS05003", 25);
    check_xml_diag("OLS05004", 36);
    check_xml_diag("OLS05005", 38);
    check_xml_diag("OLS05006", 39);
    check_xml_diag("OLS05007", 40);
    check_xml_diag("OLS05008", 41);
    check_xml_diag("OLS05009", 43);
    check_xml_diag("OLS05010", 42);
    check_xml_diag("OLS05011", 45);
    check_xml_diag("OLS05012", 48);
    check_xml_diag("OLS05013", 50);
    check_xml_diag("OLS05014", 51);
    check_xml_diag("OLS05015", 53);
    check_xml_diag("OLS05016", 54);
    check_xml_diag("OLS05017", 55);
    check_xml_diag("OLS05018", 56);
    check_xml_diag("OLS05019", 57);
    check_xml_diag("OLS05020", 58);
    check_xml_diag("OLS05021", 59);
    check_xml_diag("OLS05022", 60);
    check_xml_diag("OLS05023", 61);
    check_xml_diag("OLS05024", 62);
    check_xml_diag("OLS05025", 63);
    check_xml_diag("OLS05026", 64);
    check_xml_diag("OLS05027", 65);
    check_xml_diag("OLS05028", 66);
    check_xml_diag("OLS05029", 67);
    check_xml_diag("OLS05030", 68);
    check_xml_diag("OLS05031", 69);
    check_xml_diag("OLS05032", 70);
    check_xml_diag("OLS05036", 71);
    check_xml_diag("OLS05037", 72);
    check_xml_diag("OLS05033", 74);
    check_xml_diag("OLS05034", 75);
    check_xml_diag("OLS05035", 76);
    // OLS05040 - OLS05043 are never used (removed/replaced)
    check_xml_diag("OLS05044", 77);
    check_xml_diag("OLS05044", 78);
    check_xml_diag("OLS05045", 79);
    check_xml_diag("OLS05046", 80);
    check_xml_diag("OLS05047", 81);
    check_xml_diag("OLS05038", 82);
    check_xml_diag("OLS05048", 83);
    check_xml_diag("OLS05039", 84);
    check_xml_diag("OLS05051", 85);
    check_xml_diag("OLS05052", 86);
    check_xml_diag("OLS05053", 87);
    check_xml_diag("OLS05054", 88);
    check_xml_diag("OLS05055", 89);
    check_xml_diag("OLS05056", 90);
    check_xml_diag("OLS05057", 91);
    check_xml_diag("OLS05055", 92);
    check_xml_diag("OLS05056", 93);
}

#[test]
fn test_ols05000s_manifest() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let bikes_py_path = test_addons_path.join("module_for_diagnostics").join("__manifest__.py");
    assert!(PathBuf::from(&bikes_py_path).exists(), "Test file does not exist: {}", bikes_py_path.display());
    let bikes_py_diagnostics = get_diagnostics_for_path(&mut session, &bikes_py_path.sanitize());
    let doc_diags = get_diagnostics_test_comments(&mut session, &bikes_py_path.sanitize());
    verify_diagnostics_against_doc(bikes_py_diagnostics, doc_diags); // OLS05049 & OLS05050
}