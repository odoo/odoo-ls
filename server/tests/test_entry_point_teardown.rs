mod setup;

use odoo_ls_server::core::symbols::symbol_keys::KeyValidator;
use odoo_ls_server::utils::PathSanitizer;
use std::env;

/// Regression test for the `EntryPoint` -> `EntryPointKey` migration: dropping a custom
/// entry point must free both its `EntryPointKey` slot and its owned `RootSymbol`/`RootKey`
/// slot, not just remove it from `EntryPointMgr`'s `Vec`. Before the atomic
/// `SymbolTable::drop_entry_point` was introduced, the root and the entry point were two
/// independently-owned `Rc`s kept in sync only by convention, so a bug in one of the two
/// removal call sites could leak one half while dropping the other.
#[test]
fn test_custom_entry_point_teardown_frees_entry_and_root() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/follow_ref.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let entry_key = *session
        .sync_odoo
        .entry_point_mgr
        .custom_entry_points
        .iter()
        .find(|&&ep| session.ep_mgr()[ep].path == path)
        .expect("custom entry point should have been created for the path");
    let root_key = session.ep_mgr()[entry_key].root;

    assert!(session.ep_mgr().is_key_valid(entry_key), "entry point key should be valid right after creation");
    assert!(session.st().is_key_valid(root_key), "root key should be valid right after creation");

    session
        .sync_odoo
        .entry_point_mgr
        .remove_entries_with_path(&mut session.sync_odoo.symbol_table, &path);

    assert!(
        !session.sync_odoo.entry_point_mgr.custom_entry_points.contains(&entry_key),
        "entry point should be removed from EntryPointMgr's custom_entry_points"
    );
    assert!(!session.ep_mgr().is_key_valid(entry_key), "entry point slot should be freed after teardown");
    assert!(!session.st().is_key_valid(root_key), "root slot should be freed after teardown (no leak)");
}
