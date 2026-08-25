use std::path::Path;

use lsp_types::{DidCloseTextDocumentParams, DidOpenTextDocumentParams, TextDocumentIdentifier, TextDocumentItem};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};

mod setup;

// `handle_did_close` treats an untitled path as always "gone from disk"
// (`FileMgr::is_untitled`), so closing one takes the unconditional-clear
// path. Guards against a crash/regression there - the untitled `FileInfo`
// itself leaking forever on close is a separate, pre-existing bug (nothing
// ever removes from `untitled_files`), out of scope here.
#[test]
fn test_untitled_buffer_close_does_not_crash() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let untitled_uri_str = "untitled:Untitled-1".to_string();
    let untitled_uri = FileMgr::pathname2uri(&untitled_uri_str);

    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: untitled_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: "def foo():\n    return 42\n".to_string(),
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);

    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&untitled_uri_str)
        .expect("FileInfo for the untitled buffer should exist once opened");
    assert!(file_info.borrow().opened);
    drop(file_info);
    assert!(SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&untitled_uri_str)).is_some());

    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: untitled_uri },
    });

    assert!(
        !session.sync_odoo.entry_point_mgr.borrow().untitled_entry_points.iter().any(|ep| ep.borrow().path == untitled_uri_str),
        "the untitled entry point must still be removed on close"
    );
}
