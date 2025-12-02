mod setup;
mod test_utils;
use odoo_ls_server::core::file_mgr::FileInfo;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol::Symbol;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;

#[test]
fn test_follow_ref() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/properties.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points .len() == 1);
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, &PathBuf::from(&path))
        .expect("Failed to get file symbol");

    test_variable_type_resolution(session, file_info, file_symbol);
}

fn test_variable_type_resolution(
    mut session: SessionInfo<'_>,
    file_info: Rc<RefCell<FileInfo>>,
    file_symbol: Rc<RefCell<Symbol>>,
) {
    for (var, (line, character), expected_type) in vec![
        ("a", (9, 0), "TestClass"),
        ("b", (10, 0), "TestClass"),
        ("b", (12, 4), "int"),
        ("b", (13, 0), "(int | TestClass)"),
        ("c", (14, 0), "(int | TestClass)"),
        ("c", (16, 4), "str"),
        ("c", (17, 0), "(str | int | TestClass)"),
        ("d", (18, 0), "(str | int | TestClass)"),
    ] {
        let hover =
            test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, line, character)
                .expect(&format!("Should get hover text for {}", var));
        assert!(
            hover.contains(format!("{var}: {expected_type}").as_str()),
            "Hover over '{}' should show type '{}'. Got: {}", var, expected_type, hover);
    }
}
