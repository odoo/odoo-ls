mod setup;
mod test_utils;

use odoo_ls_server::core::file_mgr::FileInfo;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::{SourceFileKey, SymbolKey};
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::cell::RefCell;
use std::env;
use std::path::Path;
use std::rc::Rc;
use test_utils::get_resolved_symbols_at_position;

/// Sets up a session with `type_hints.py` loaded and resolves `MyClass`, then calls `f` with
/// the session, file info, file symbol, and `MyClass` symbol.
fn with_type_hints_fixture<F>(f: F)
where
    F: FnOnce(&mut SessionInfo, &Rc<RefCell<FileInfo>>, SourceFileKey, SymbolKey),
{
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/type_hints.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    let my_class = session.sync_odoo.get_symbol(path.as_str(), (&[], &["MyClass"]), u32::MAX);
    assert!(!my_class.is_empty(), "MyClass should be found in the test file");
    let my_class = my_class[0];

    f(&mut session, &file_info, file_symbol, my_class);
}

/// Test that a function with a return type hint and `pass` body is recognized as returning
/// the hinted type. The body provides no inferrable return value, so the hint must be used.
#[test]
fn test_function_return_type_hint() {
    with_type_hints_fixture(|session, file_info, file_symbol, my_class| {
        // Line 14: `result` — the standalone reference to the variable assigned from get_my_class().
        // get_my_class() has `-> MyClass` and body `pass`, so the return type must come from
        // the type hint rather than any inferred return statement.
        let resolved = get_resolved_symbols_at_position(session, file_symbol, file_info, 14, 0);
        assert!(
            resolved.len() == 1 && resolved[0] == my_class,
            "result of get_my_class() should resolve to MyClass via return type hint, got: {:?}",
            resolved.iter().map(|&s| session.st().name(s).to_string()).collect::<Vec<_>>()
        );
    });
}

/// Test that the variable bound in a `with ... as <name>` statement has the type returned
/// by the context manager's `__enter__` method.
#[test]
fn test_with_statement_type_from_enter() {
    with_type_hints_fixture(|session, file_info, file_symbol, my_class| {
        // Line 17: `    ctx` — the bound variable from `with MyContextManager() as ctx:`.
        // MyContextManager.__enter__ has `-> MyClass`, so ctx should resolve to MyClass.
        let resolved = get_resolved_symbols_at_position(session, file_symbol, file_info, 17, 4);
        assert!(
            resolved.len() == 1 && resolved[0] == my_class,
            "ctx in with statement should resolve to MyClass via __enter__ return type, got: {:?}",
            resolved.iter().map(|&s| session.st().name(s).to_string()).collect::<Vec<_>>()
        );
    });
}
