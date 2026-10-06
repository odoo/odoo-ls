mod setup;

use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;
use std::env;

/// Regression test: deleting a file that is in the workspace (or a custom entry point)
/// used to panic. `FileMgr::delete_entry` called `SymbolTable::drop_file_info` (freeing the
/// `FileInfoKey` slot) *before* using that same key to clear and publish diagnostics,
/// so the subsequent `session.st_mut()[to_del]` indexed an already-removed slotmap entry.
#[test]
fn test_delete_file_path_does_not_panic() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/follow_ref.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    // close the file so it gets actually deleted
    let file_info = session.file_mgr().get_file_info(&path).unwrap();
    session.file_mgr_mut()[file_info].opened = false;

    // Sanity check: the file is reachable via a custom entry point, so `delete_entry`
    // takes the diagnostics-clearing branch that triggered the panic.
    assert!(SyncOdoo::is_in_workspace_or_entry(&session, &path));
    assert!(session.file_mgr().get_file_info(&path).is_some());

    FileMgr::delete_file_path(&mut session, &path);

    assert!(
        session.file_mgr().get_file_info(&path).is_none(),
        "file info should be gone from the name index after delete"
    );
}
