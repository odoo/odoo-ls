use std::path::PathBuf;

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::diag_on_line};

/// OLS03029: assigning on a recordset an attribute that is not a field nor a member of the model
/// (`self.cache = {}`). Reported on the declaration, not on the usages.
#[test]
fn test_ols03029_ext_symbol_on_recordset() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1")
        .join("models")
        .join("instance_attr_probe.py")
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);

    // Line 10: self.cache = {} → `cache` is not a field nor a member of the model, assigned on a recordset
    let on_line = diag_on_line(&diagnostics, 10);
    assert_eq!(on_line.len(), 1, "expected exactly one diagnostic on self.cache = {{}}, got: {:?}", on_line);
    let diag = on_line[0];
    assert!(
        matches!(&diag.code, Some(NumberOrString::String(c)) if c == "OLS03029"),
        "expected OLS03029 on self.cache = {{}}, got: {:?}",
        diag.code
    );
    assert_eq!(diag.severity, Some(DiagnosticSeverity::WARNING), "OLS03029 should default to Warning");
    assert!(diag.message.contains("'cache'"), "expected the message to mention the attribute, got: {}", diag.message);

    // Line 13: return self.cache → the usage is not reported, only the declaration
    let on_line = diag_on_line(&diagnostics, 13);
    assert!(on_line.is_empty(), "expected no diagnostic on the usage of self.cache, got: {:?}", on_line);

    // Line 16: self.value = 1 → `value` is a field
    let on_line = diag_on_line(&diagnostics, 16);
    assert!(on_line.is_empty(), "expected no diagnostic on the assignment of a field, got: {:?}", on_line);

    // Line 9: paths.memo → instance attribute of a class that is not a model
    let on_line = diag_on_line(&diagnostics, 9);
    assert!(on_line.is_empty(), "expected no diagnostic on paths.memo, got: {:?}", on_line);
}

/// OLS03029 is not raised in tests, where setting attributes on recordsets is a common and
/// controlled practice.
#[test]
fn test_ols03029_not_in_tests() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1")
        .join("tests")
        .join("test_instance_attr_probe.py")
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);

    // Line 7: probe.cache = {...} → attribute set on a recordset, but in a test
    let on_line: Vec<_> = diag_on_line(&diagnostics, 7).into_iter()
        .filter(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "OLS03029"))
        .collect();
    assert!(on_line.is_empty(), "expected no OLS03029 in tests, got: {:?}", on_line);
}

/// OLS03029 is not raised on AbstractModels, that are often mixins using fields declared by the models
/// inheriting from them.
#[test]
fn test_ols03029_not_on_abstract_model() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1")
        .join("models")
        .join("abstract_attr_probe.py")
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);

    // Line 9: self.concrete_value = 1 → attribute set on a recordset of an AbstractModel
    let on_line = diag_on_line(&diagnostics, 9);
    assert!(on_line.is_empty(), "expected no diagnostic on an AbstractModel, got: {:?}", on_line);
}
