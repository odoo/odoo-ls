use std::env;
use std::path::PathBuf;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;
mod test_utils;

#[test]
fn test_search_eval_hook() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let test_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_1/models/base_test_models.py")
        .sanitize();

    let Some(file_symbol) =
        SyncOdoo::get_symbol_of_opened_file(&mut session, &PathBuf::from(&test_file))
    else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();

    // Hover over closing parenthesis in call to `search` to verify its return type
    let hover_text = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 33, 60)
        .expect("Should get hover text for return type of search");

    assert!(
        hover_text.contains("BaseTestModel"),
        "Hover over 'search' return type should show 'BaseTestModel' class. Got: {}",
        hover_text
    );
}
