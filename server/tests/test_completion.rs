use std::env;
use std::path::Path;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let items = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|i| i.label).collect()
}

#[test]
fn test_completions() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    test_depends_kwarg_nested_field_completion(&mut session);
    test_lambda_is_not_a_member(&mut session);
    test_compute_sql_kwarg_method_completion(&mut session);
}
 
/// `fields.Char(compute="...", depends=["partner_id.disp"])`: the `depends` kwarg should
/// offer nested-field completion on the last dotted segment, exactly like `related=`.
fn test_depends_kwarg_nested_field_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {}", test_file);

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Line `    partner_display_name_dep = fields.Char(compute="_compute_partner_display_name_dep", depends=["partner_id.disp"])`
    // (0-indexed line 79), cursor right after "disp" inside the string.
    let response = CompletionFeature::autocomplete(session, file_symbol, file_info, None, 79, 113);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "display_name"), "Expected display_name to be suggested for the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "create_uid"), "create_uid does not match the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "id"), "id does not match the 'disp' prefix, got: {:?}", labels);
}

/// `default=lambda self: ...` stores a `<lambda>` child on the class, but it is not a member:
/// `self.` must never offer it.
fn test_lambda_is_not_a_member(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let disk_text = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    // Open the buffer with the dot typed after `return self`: the completed attribute name is
    // then empty, so every member of LambdaDefaultModel is offered.
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&test_file),
            language_id: "python".to_string(),
            version: 1,
            text: disk_text.replace("return self\n", "return self.\n"),
        }
    };
    Odoo::handle_did_open(session, did_open_params);

    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // `return self.` is line 25 of the fixture (0-indexed 24), cursor after the dot.
    let response = CompletionFeature::autocomplete(session, file_symbol, file_info, None, 24, 20);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "company_id"), "Expected the model fields to be suggested after 'self.', got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "<lambda>"), "<lambda> is not a member and must not be suggested, got: {:?}", labels);
}

/// `fields.Integer(compute_sql="...")`: the kwarg offers method completion, but only from 19.1 on
fn test_compute_sql_kwarg_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor inside the compute_sql value of `test_int = fields.Integer(..., compute_sql="...")`
    let initial_version = session.sync_odoo.version;
    session.sync_odoo.version = OdooVersion::new(19, 1, 0);
    let gated_in = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 8, 80));
    assert!(gated_in.iter().any(|l| l == "_compute_something"), "Expected _compute_something to be suggested for compute_sql, got: {:?}", gated_in);
    session.sync_odoo.version = OdooVersion::new(19, 0, 0);
    let gated_out = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 8, 80));
    assert!(!gated_out.iter().any(|l| l == "_compute_something"), "Expected no method completion for compute_sql before 19.1, got: {:?}", gated_out);
    // The session is shared with the other completion tests, leave the version as it was found
    session.sync_odoo.version = initial_version;
}
