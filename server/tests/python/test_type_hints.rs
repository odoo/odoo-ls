use crate::common;

use odoo_ls_server::core::file_mgr::FileInfoKey;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::{SourceFileKey, SymbolKey};
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::Path;
use common::get_resolved_symbols_at_position;

/// Loads `type_hints.py` into `session` and resolves `MyClass`, then calls `f` with
/// the session, file info, file symbol, and `MyClass` symbol.
fn with_type_hints_fixture<F>(session: &mut SessionInfo, f: F)
where
    F: FnOnce(&mut SessionInfo, FileInfoKey, SourceFileKey, SymbolKey),
{
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/type_hints.py")
        .sanitize();
    common::prepare_custom_entry_point(session, path.as_str());

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&path))
        .expect("Failed to get file symbol");

    let my_class = session.sync_odoo.get_symbol(path.as_str(), (&[], &["MyClass"]), u32::MAX);
    assert!(!my_class.is_empty(), "MyClass should be found in the test file");
    let my_class = my_class[0];

    f(session, file_info, file_symbol, my_class);
}

/// Test that a function with a return type hint and `pass` body is recognized as returning
/// the hinted type. The body provides no inferrable return value, so the hint must be used.
pub fn test_function_return_type_hint(mut session: SessionInfo) {
    with_type_hints_fixture(&mut session, |session, file_info, file_symbol, my_class| {
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
pub fn test_with_statement_type_from_enter(mut session: SessionInfo) {
    with_type_hints_fixture(&mut session, |session, file_info, file_symbol, my_class| {
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
