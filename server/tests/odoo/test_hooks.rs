use std::env;
use std::path::Path;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::common;

pub fn test_search_eval_hook(mut session: SessionInfo) {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_1/models/base_test_models.py")
        .sanitize();

    let Some(file_symbol) =
        SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
    else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();

    // Hover over closing parenthesis in call to `search` to verify its return type
    let hover_text = common::get_hover_markdown(&mut session, file_symbol, file_info, 33, 60)
        .expect("Should get hover text for return type of search");

    assert!(
        hover_text.contains("BaseTestModel"),
        "Hover over 'search' return type should show 'BaseTestModel' class. Got: {}",
        hover_text
    );
}
