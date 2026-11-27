use std::fs;
use std::path::Path;

use lsp_server::Message;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::common::{build_once_run_isolated, setup_server};

fn assign_py() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/python/expressions/assign.py").sanitize()
}

fn count_a(session: &SessionInfo) -> usize {
    session.sync_odoo.get_symbol(&assign_py(), (&[], &["a"]), u32::MAX).len()
}

/// `SyncOdoo::init` sends "End of initialization" last: a case must find it at the end of its queue.
#[track_caller]
fn assert_queue_holds_init_messages(session: &mut SessionInfo) {
    let messages: Vec<Message> = std::iter::from_fn(|| session._consume_message()).collect();
    let last = messages.last().map(|m| format!("{m:?}")).unwrap_or_default();
    assert!(last.contains("End of initialization"), "{} messages, the last one is: {last}", messages.len());
}

fn test_open_assign_py(mut session: SessionInfo) {
    assert_queue_holds_init_messages(&mut session);
    let path = assign_py();
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: FileMgr::pathname2uri(&path),
            language_id: "python".to_string(),
            version: 1,
            text: fs::read_to_string(&path).expect("unable to read assign.py"),
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    assert_eq!(count_a(&session), 1);
}

fn test_assign_py_is_unknown(mut session: SessionInfo) {
    assert_queue_holds_init_messages(&mut session);
    assert!(session.file_mgr().get_file_info(&assign_py()).is_none());
    assert_eq!(count_a(&session), 0);
}

#[test]
fn test_build_once_run_isolated() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_open_assign_py,
        &test_assign_py_is_unknown,
        // the last case runs on the original server, not on a copy
        &test_assign_py_is_unknown,
    ]);
}
