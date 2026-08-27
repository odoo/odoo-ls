use std::env;
use std::path::Path;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::features::completion::CompletionFeature;
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

/// `fields.Char(compute="...", depends=["partner_id.disp"])`: the `depends` kwarg should
/// offer nested-field completion on the last dotted segment, exactly like `related=`.
#[test]
fn test_depends_kwarg_nested_field_completion() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {}", test_file);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Line `    partner_display_name_dep = fields.Char(compute="_compute_partner_display_name_dep", depends=["partner_id.disp"])`
    // (0-indexed line 79), cursor right after "disp" inside the string.
    let response = CompletionFeature::autocomplete(&mut session, file_symbol, &file_info, None, 79, 113);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "display_name"), "Expected display_name to be suggested for the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "create_uid"), "create_uid does not match the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "id"), "id does not match the 'disp' prefix, got: {:?}", labels);
}
