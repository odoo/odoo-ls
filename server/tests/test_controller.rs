use std::env;
use std::path::Path;

use odoo_ls_server::core::file_mgr::FileInfoKey;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::constants::{BuildStatus, BuildSteps, SymType};
use odoo_ls_server::core::evaluation::HookName;
use odoo_ls_server::core::symbols::storage::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::{SourceFileKey, SymbolKey};
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
        session.sync_odoo.model_mgr.get_model("res.partner").is_some(),
        "ResPartner should exists in models"
    );
    // check that test_request_type is built
    let methods = session.st().get_symbol(SymbolKey::from(file_symbol), (&[], &["TestController", "test_request_type"]), u32::MAX);
    assert_eq!(methods.len(), 1, "expected TestController.test_request_type");
    let method = methods[0].unwrap_buildable_key();
    assert_eq!(
        session.st().build_status(method, BuildSteps::VALIDATION),
        BuildStatus::DONE,
        "TestController.test_request_type should be validated"
    );


    // request.env must resolve to Odoo's Environment class (odoo.api < 18.1, odoo.orm.environments >= 18.1),
    // whose __getitem__ carries the EvalEnvGetItem hook that resolves env["model.name"] subscripts.
    let syms = test_utils::get_resolved_symbols_at_position(session, file_symbol, file_info, 14, 22);
    let env_class = syms.iter().copied().find(|&sym| {
        sym.typ() == SymType::CLASS && {
            let tree = SymbolTable::get_local_tree(session, sym);
            tree == (&["odoo", "api"], &["Environment"])
                || tree == (&["odoo", "orm", "environments"], &["Environment"])
        }
    });
    let Some(env_class) = env_class else {
        panic!(
            "request.env should resolve to odoo's Environment class. Got: {:?}",
            syms.iter().map(|&s| SymbolTable::get_local_tree(session, s)).collect::<Vec<_>>()
        );
    };
    let get_items = session.st().get_symbol(env_class, (&[] as &[&str], &["__getitem__"]), u32::MAX);
    assert_eq!(get_items.len(), 1, "Environment should define __getitem__");
    let has_env_hook = session.st().evaluations(get_items[0]).is_some_and(|evals| {
        evals.iter().any(|eval| {
            eval.symbol.get_symbol_hook.as_ref().is_some_and(|hook| hook.name == HookName::EvalEnvGetItem)
        })
    });
    assert!(has_env_hook, "Environment.__getitem__ should carry the EvalEnvGetItem hook");

    // Hover on the closing bracket of request.env['res.partner'] should show the Partner/ResPartner class
    let bracket_hover = test_utils::get_hover_markdown(session, file_symbol, file_info, 17, 43)
        .expect("Should get hover text on the bracket of request.env['res.partner']");
    assert!(
        bracket_hover.contains(partner_class),
        "Hover on request.env['res.partner'] should show {} class. Got: {}",
        partner_class,
        bracket_hover
    );

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
