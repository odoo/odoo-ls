use std::env;
use std::path::Path;

use lsp_types::{DidCloseTextDocumentParams, DidOpenTextDocumentParams, TextDocumentIdentifier, TextDocumentItem};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

// `in_workspace` only checks a file's immediate parent, so a file under a
// bare, `__init__.py`-less directory (e.g. `pkg/models/point.py`) ends up
// `in_workspace = false` even though it's real and internal (not external
// either). Since `handle_did_close` clears only on "gone from disk" or
// "external", a file like this must just behave like any other ordinary
// tracked file - kept on close.
#[test]
fn test_nested_bare_dir_file_is_kept_on_close_even_though_not_flagged_in_workspace() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let pkg_dir = env::current_dir().unwrap().join("tests/data/nested_dir_race/pkg").sanitize();
    let pkg_init = Path::new(&pkg_dir).join("__init__.py").sanitize();
    let point_path = Path::new(&pkg_dir).join("models").join("point.py").sanitize();
    let point_content = std::fs::read_to_string(&point_path).unwrap();
    let point_uri = FileMgr::pathname2uri(&point_path);

    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &pkg_dir, &pkg_init);
    BuildScheduler::process_rebuilds(&mut session, false);

    let point_syms = session.sync_odoo.get_symbol(&pkg_dir, (&["models", "point"], &[]), u32::MAX);
    assert_eq!(point_syms.len(), 1, "expected exactly one symbol for pkg.models.point");
    let point_sym = point_syms[0];
    assert!(!session.st().is_external(point_sym));
    assert!(!session.st().in_workspace(point_sym), "immediate parent is a bare dir, in_workspace is never computed for it");

    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: point_content,
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);

    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: point_uri },
    });

    assert!(
        session.file_mgr().get_file_info(&point_path).is_some(),
        "closing must not evict the FileInfo of a live, valid, internal, non-external file"
    );
    assert!(!session.sync_odoo.get_symbol(&pkg_dir, (&["models", "point"], &[]), u32::MAX).is_empty());
}
