mod setup;
mod test_utils;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::Path;

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let items = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|i| i.label).collect()
}

/// `self.x = ...` in a method injects `x` into the class as an ext symbol, built with the
/// function ARCH/ARCH_EVAL: it must be completed and evaluated on the class instances.
#[test]
fn test_instance_attributes() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/instance_attributes.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    // `        self.` in `Test.__init__`, where the attributes are assigned
    let completions = labels(CompletionFeature::autocomplete(&mut session, file_symbol, &file_info, None, 5, 13));
    assert!(completions.iter().any(|l| l == "value"), "Expected 'value' after 'self.', got: {:?}", completions);
    assert!(completions.iter().any(|l| l == "count"), "Expected 'count' after 'self.', got: {:?}", completions);

    // `        return self.cou` in another method of the class
    let completions = labels(CompletionFeature::autocomplete(&mut session, file_symbol, &file_info, None, 8, 23));
    assert!(completions.iter().any(|l| l == "count"), "Expected 'count' in another method, got: {:?}", completions);

    // `t.count` at module level is evaluated from the assignment in `__init__`
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 21, 3).unwrap_or_default();
    assert!(hover.contains("count") && hover.contains("int"), "hover on t.count should show an int, got: {hover:?}");

    // a declared class attribute takes precedence over the instance attribute
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 18, 21).unwrap_or_default();
    assert!(hover.contains("str") && !hover.contains("int"), "hover on self.label should show the class attribute, got: {hover:?}");
}
