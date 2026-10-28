use std::path::{Path, PathBuf};

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{diag_on_line, get_hover_markdown}};

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

/// OLS03029 and OLS02003 are not raised on AbstractModels, that are often mixins using fields declared by the models
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

    // Line 13: return self.concrete_value → no OLS02003 on the usage either
    let on_line = diag_on_line(&diagnostics, 13);
    assert!(on_line.is_empty(), "expected no OLS02003 on an AbstractModel, got: {:?}", on_line);
}

/// OLS02003 is raised only if no candidate type of the base has the attribute.
#[test]
fn test_ols02003_union_of_candidates() {
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

    // Line 26: self.pick(True).value → `value` is a field of only one of the candidates
    let on_line = diag_on_line(&diagnostics, 26);
    assert!(on_line.is_empty(), "expected no diagnostic when one candidate has the attribute, got: {:?}", on_line);

    // Line 27: self.pick(True).not_on_any_candidate → no candidate has it, reported once
    let on_line = diag_on_line(&diagnostics, 27);
    assert_eq!(on_line.len(), 1, "expected exactly one diagnostic when no candidate has the attribute, got: {:?}", on_line);
    assert!(
        matches!(&on_line[0].code, Some(NumberOrString::String(c)) if c == "OLS02003"),
        "expected OLS02003, got: {:?}",
        on_line[0].code
    );
}

/// `Form(record)` keeps the model of `record`: the fields of the model are found on the form (the x2many ones
/// as `M2MProxy` / `O2MProxy`), and assigning them does not declare attributes on the Form class.
#[test]
fn test_ols02003_form_fields() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1")
        .join("tests")
        .join("test_form_proxy.py")
        .sanitize();
    // test files are not loaded by the initial build: open it
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&path),
            language_id: "python".to_string(),
            version: 1,
            text: std::fs::read_to_string(&path).expect("Test file does not exist"),
        }
    };
    Odoo::handle_did_open(&mut session, did_open_params);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let not_found: Vec<_> = diagnostics.iter()
        .filter(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "OLS02003"))
        .collect();
    assert!(not_found.is_empty(), "expected no OLS02003 on the fields of a Form, got: {:?}", not_found);

    // Lines 9 and 10: `model_form.partner_ids = ...`, `model_form.parent_id = ...` set fields of the model
    let odoo_path = Path::new(&std::env::var("COMMUNITY_PATH").unwrap()).sanitize();
    let form_class = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "tests", "form"], &["Form"]), u32::MAX);
    assert_eq!(form_class.len(), 1, "Form not found");
    for field_name in ["partner_ids", "parent_id"] {
        assert!(session.st().get_ext_symbol(form_class[0], field_name).is_empty(), "{field_name} should not be declared on Form");
    }

    // Line 11: `model_form.partner_ids` is a M2MProxy
    let file_info = session.file_mgr().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path)).unwrap();
    let hover = get_hover_markdown(&mut session, file_symbol, file_info, 11, 22).unwrap_or_default();
    assert!(hover.contains("M2MProxy"), "expected model_form.partner_ids to be a M2MProxy, got: {hover}");

    // Lines 18 and 22: `with Form(...) as f` keeps the model
    let hover = get_hover_markdown(&mut session, file_symbol, file_info, 18, 16).unwrap_or_default();
    assert!(hover.contains("M2MProxy"), "expected f.partner_ids to be a M2MProxy, got: {hover}");
    let hover = get_hover_markdown(&mut session, file_symbol, file_info, 22, 20).unwrap_or_default();
    assert!(hover.contains("O2MProxy"), "expected f.child_ids to be a O2MProxy, got: {hover}");
}
