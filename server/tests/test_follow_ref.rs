mod setup;
mod test_utils;
use odoo_ls_server::core::file_mgr::FileInfo;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::{SourceFileKey};
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::cell::RefCell;
use std::env;
use std::path::Path;
use std::rc::Rc;
use test_utils::get_resolved_symbols_at_position;

#[test]
fn test_follow_ref() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/follow_ref.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    test_variable_type_resolution(&mut session, &file_info, file_symbol);
}

fn test_variable_type_resolution(
    session: &mut SessionInfo<'_>,
    file_info: &Rc<RefCell<FileInfo>>,
    file_symbol: SourceFileKey,
) {
    let test_class = session.st().get_sub_symbol(file_symbol.into(), "TestClass", u32::MAX).symbols[0];
    let int_type = session.sync_odoo.get_symbol("", (&["builtins"], &["int"]), u32::MAX)[0];
    let str_type = session.sync_odoo.get_symbol("", (&["builtins"], &["str"]), u32::MAX)[0];

    // Test cases: (var_name, (line, character), expected_types)
    let test_cases = [
        ("a", (3, 0), vec![&test_class]),
        ("b", (4, 0), vec![&test_class]),
        ("b", (6, 4), vec![&int_type]),
        ("b", (7, 0), vec![&int_type, &test_class]),
        ("c", (8, 0), vec![&int_type, &test_class]),
        ("c", (10, 4), vec![&str_type]),
        ("c", (11, 0), vec![&str_type, &int_type, &test_class]),
        ("d", (12, 0), vec![&str_type, &int_type, &test_class]),
    ];
    for (var_name, (line, character), expected_types) in test_cases {
        let resolved_types = get_resolved_symbols_at_position( session, file_symbol, file_info, line, character);

        assert!(
            resolved_types.len() == expected_types.len() &&
            resolved_types.iter().zip(&expected_types).all(|(a, &b)| a == b ),
            "Variable '{}' at line {} should have types {:?}, but got {:?}",
            var_name, line,
            expected_types.iter().map(|&s| session.st().name(*s).to_string()).collect::<Vec<_>>(),
            resolved_types.iter().map(|&s| session.st().name(s).to_string()).collect::<Vec<_>>()
        );
    }
}
