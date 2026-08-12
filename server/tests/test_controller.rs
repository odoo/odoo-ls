use std::env;
use std::path::Path;

use odoo_ls_server::core::file_mgr::FileInfoKey;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::{SourceFileKey};
use odoo_ls_server::utils::PathSanitizer;
use odoo_ls_server::threads::SessionInfo;

mod setup;
mod test_utils;

/// Test that odoo.http.request and request.env hooks work correctly.
/// Tests hover and definition.
#[test]
fn test_controller() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_1/controllers/main.py")
        .sanitize();

    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();

    test_request_type_hover(&mut session, file_symbol, file_info);
    test_request_env_definition(&mut session, file_symbol, file_info);
    test_request_env_subscript(&mut session, file_symbol, file_info);
}

/// Test that hovering over 'request' shows Request class
/// and that hovering over 'request.env' shows Environment type
fn test_request_type_hover(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: FileInfoKey
) {
    // Test 1: Hover over 'request' variable (line 11: req = request)
    // Should show Request class
    let hover_text = test_utils::get_hover_markdown(session, file_symbol, file_info, 11, 14)
        .expect("Should get hover text for request");

    assert!(
        hover_text.contains("Request"),
        "Hover over 'request' should show Request class. Got: {}",
        hover_text
    );

    // Test 2: Hover over 'request.env' (line 14: env = request.env)
    // Should show Environment | None
    let hover_text = test_utils::get_hover_markdown(session, file_symbol, file_info, 14, 21)
        .expect("Should get hover text for request.env");

    assert!(
        hover_text.contains("Environment"),
        "Hover over 'request.env' should show Environment type. Got: {}",
        hover_text
    );
    assert!(
        hover_text.contains("None"),
        "Hover over 'request.env' should show None. Got: {}",
        hover_text
    );
}

/// Test that request.env provides correct definitions
fn test_request_env_definition(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: FileInfoKey
) {
    // Test 1: Go-to-definition on 'request' import (line 3: from odoo.http import request)
    // Should navigate to request variable in odoo.http
    let definitions = test_utils::get_definition_locs(session, file_symbol, file_info, 2, 22);

    assert!(
        !definitions.is_empty(),
        "Should find definition for 'request' import"
    );

    // Verify it points to the correct file
    let target_uri = &definitions[0].target_uri.to_string();
    assert!(
        target_uri.contains("odoo/http"),
        "Definition should point to http or requestlib. Got: {:?}",
        target_uri
    );

    // Test 2: Go-to-definition on 'request.env' (line 15: env = request.env)
    let definitions = test_utils::get_definition_locs(session, file_symbol, file_info, 14, 22);
    assert!(
        !definitions.is_empty(),
        "Should find definition for 'request.env'"
    );
}

/// Test that request.env["model_name"] resolves to Model instance
fn test_request_env_subscript(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: FileInfoKey
) {
    let partner_class = test_utils::PARTNER_CLASS_NAME(session.sync_odoo.version);

    // Test 1: Hover over 'partner' variable (line 18: partner = request.env['res.partner'])
    // Should show Partner/ResPartner class
    let hover_text = test_utils::get_hover_markdown(session, file_symbol, file_info, 17, 8)
        .expect("Should get hover text for 'partner' variable");

    assert!(
        hover_text.contains(partner_class),
        "Hover over 'partner' should show {} class. Got: {}",
        partner_class,
        hover_text
    );

    // Test 2: Hover over .search on request.env['res.partner'].search (line 19)
    // Should display markdown content for the search method
    let hover_text = test_utils::get_hover_markdown(session, file_symbol, file_info, 18, 46)
        .expect("Should get hover text for .search method");

    assert!(
        hover_text.contains("def search"),
        "Hover over .search should show its definition. Got: {}",
        hover_text
    );
}
