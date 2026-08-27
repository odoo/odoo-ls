use std::env;
use std::fs;
use std::path::Path;

use lsp_types::{DidChangeWatchedFilesParams, FileChangeType, FileEvent};
use odoo_ls_server::constants::OYarn;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::core::symbols::symbol_keys::SymbolKey;
use odoo_ls_server::utils::PathSanitizer;
use odoo_ls_server::Sy;

mod setup;

// Regression test for the panic at odoo.rs:2414 (`Option::unwrap()` on `None`) in
// `search_symbols_to_rebuild`'s `addons_entry_points` loop: deleting an addon's
// directory unloads its symbol, but `clean_entries` never dropped the now-stale
// entry, so the next create/change event under it hit an empty `get_symbol()`.
// Fixed by making `clean_entries` actually drop it, and by re-registering the entry
// (`restore_addon_entry`) if its path is created again.
#[test]
fn test_addon_entry_losing_its_symbol_does_not_panic() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let _ = session.test_drain_restart_notifications(); // discard whatever init itself sent

    let fixture_root = env::current_dir().unwrap().join("tests/data/addon_entry_lost_symbol").sanitize();
    let addons_dir = Path::new(&fixture_root).join("odoo").join("addons").sanitize();

    // this test deletes and recreates the fixture, so build it fresh each run
    let _ = fs::remove_dir_all(&fixture_root);
    fs::create_dir_all(&addons_dir).unwrap();

    // stand-in for the real "odoo" main entry point
    let main_sym = EntryPointMgr::set_main_entry(&mut session, fixture_root.clone()).unwrap();
    session.st_mut().set_is_external(main_sym, false);
    let main_entry = session.sync_odoo.entry_point_mgr.borrow().main_entry_point.as_ref().unwrap().clone();
    session.sync_odoo.main_entry_tree = main_entry.borrow().tree.clone();

    // stand-in for the real "odoo.addons" namespace
    EntryPointMgr::create_dir_symbols_from_path_to_entry(&mut session, Path::new(&addons_dir), main_entry.clone());
    EntryPointMgr::add_entry_to_addons(&mut session, addons_dir.clone(), main_entry.clone(), vec![Sy!("odoo"), Sy!("addons")]);

    session.sync_odoo.config.set_string_list(odoo_ls_server::core::config::ConfigKey::AddonsPaths, vec![addons_dir.clone()]);

    let entry = session.sync_odoo.entry_point_mgr.borrow().addons_entry_points.last().unwrap().clone();
    assert!(
        matches!(entry.borrow().get_symbol(session.st()), Some(SymbolKey::Namespace(_))),
        "sanity check: the addon entry should resolve to a Namespace symbol right after creation"
    );

    // delete the whole addons directory and notify the server, like a file watcher would
    fs::remove_dir_all(&addons_dir).unwrap();
    Odoo::handle_did_change_watched_files(&mut session, DidChangeWatchedFilesParams {
        changes: vec![FileEvent { uri: FileMgr::pathname2uri(&addons_dir), typ: FileChangeType::DELETED }],
    });

    assert!(
        session.sync_odoo.entry_point_mgr.borrow().addons_entry_points.is_empty(),
        "the now-symbol-less entry should have been cleaned up instead of left dangling"
    );
    assert_eq!(
        session.test_drain_restart_notifications(), 0,
        "merely dropping the stale addon entry point should not ask the client to restart"
    );

    // recreate a module under the same (now stale) addons path
    let new_module_dir = Path::new(&addons_dir).join("new_module").sanitize();
    fs::create_dir_all(&new_module_dir).unwrap();
    let manifest_path = Path::new(&new_module_dir).join("__manifest__.py").sanitize();
    let init_path = Path::new(&new_module_dir).join("__init__.py").sanitize();
    fs::write(&manifest_path, "{'name': 'New Module'}\n").unwrap();
    fs::write(&init_path, "class Thing:\n    def method(self):\n        return 1\n").unwrap();

    Odoo::handle_did_change_watched_files(&mut session, DidChangeWatchedFilesParams {
        changes: vec![
            FileEvent { uri: FileMgr::pathname2uri(&new_module_dir), typ: FileChangeType::CREATED },
            FileEvent { uri: FileMgr::pathname2uri(&manifest_path), typ: FileChangeType::CREATED },
            FileEvent { uri: FileMgr::pathname2uri(&init_path), typ: FileChangeType::CREATED },
        ],
    });

    assert_eq!(
        session.test_drain_restart_notifications(), 1,
        "restoring the addon entry point should ask the client to restart exactly once"
    );
    assert_eq!(
        session.sync_odoo.entry_point_mgr.borrow().addons_entry_points.len(), 1,
        "the addons path should have been re-registered as a real addon entry"
    );
    assert!(
        session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.is_empty(),
        "the recreated module should not have fallen back to a disconnected custom entry"
    );
    let module = session.sync_odoo.get_symbol(&fixture_root, (&["odoo", "addons", "new_module"], &[]), u32::MAX);
    assert!(
        !module.is_empty(),
        "new_module should resolve as a real odoo.addons module again"
    );

    // and the rebuilt namespace should support full symbol resolution, not just a
    // module shell: a method inside it should resolve and evaluate normally
    let method = session.sync_odoo
        .get_symbol(&fixture_root, (&["odoo", "addons", "new_module"], &["Thing", "method"]), u32::MAX)
        .first()
        .expect("new_module.Thing.method should resolve after the namespace is rebuilt")
        .unwrap_function_key();
    SyncOdoo::ensure_func_evaluations(&mut session, method);
    assert!(!session.st()[method].evaluations.is_empty(), "method should have been built successfully");

    let _ = fs::remove_dir_all(&fixture_root);
}
