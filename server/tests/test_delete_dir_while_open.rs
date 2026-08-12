use std::env;
use std::path::Path;

use lsp_types::{
    CreateFilesParams, DeleteFilesParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, FileCreate, FileDelete, Position, Range, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, VersionedTextDocumentIdentifier,
};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::utils::PathSanitizer;

mod setup;

// Restores a fixture file's original content on drop (even on panic), for
// tests that actually delete/overwrite a tracked fixture on disk.
struct RestoreFixture { path: String, original: String }
impl Drop for RestoreFixture {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.path, &self.original);
    }
}

// Regression tests for how a deleted-while-open file's symbol, entry, and
// cache get cleared. The bug: `handle_did_delete` used to tear down the
// symbol/entry immediately even for an open file, so
// `get_symbol_of_opened_file` would find nothing and resurrect a phantom,
// disk-unchecked entry via its "not found, create one" fallback. Fix: leave
// an open file's symbol/entry/cache alone on delete; `handle_did_close`
// clears them on close, based on disk existence / externality, not a
// stored flag.
#[test]
fn test_delete_open_file_then_edit_it() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let dir = env::current_dir().unwrap().join("tests/data/deleted_dir_race").sanitize();
    let point_path = Path::new(&dir).join("point.py").sanitize();
    let point_content = std::fs::read_to_string(&point_path).unwrap();
    let point_uri = FileMgr::pathname2uri(&point_path);

    // 1. Open point.py directly.
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: point_content,
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    assert!(session.file_mgr().get_file_info(&point_path).is_some());
    assert!(session.sync_odoo.opened_files.contains(&point_path));
    let point_sym_before = session.sync_odoo.get_symbol(&point_path, (&[], &["Thing"]), u32::MAX);
    assert!(!point_sym_before.is_empty(), "point.Thing should resolve before any delete");

    // 2. point.py gets deleted while the tab stays open (no didClose).
    Odoo::handle_did_delete(&mut session, DeleteFilesParams {
        files: vec![FileDelete { uri: point_uri.to_string() }],
    });

    assert!(session.sync_odoo.opened_files.contains(&point_path), "delete must not implicitly close it");
    assert!(session.file_mgr().get_file_info(&point_path).is_some(), "FileInfo must survive");
    assert!(
        !session.sync_odoo.get_symbol(&point_path, (&[], &["Thing"]), u32::MAX).is_empty(),
        "symbol must survive the delete while open - the root-cause fix"
    );

    // 3. The user keeps typing; an ordinary didChange, never preceded by didClose.
    Odoo::handle_did_change(&mut session, DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier { uri: point_uri, version: 2 },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 0), Position::new(0, 0))),
            range_length: None,
            text: "x".to_string(),
        }],
    });

    let file_info = session.file_mgr().get_file_info(&point_path)
        .expect("FileInfo must still exist after the incremental edit");
    let contents = session.file_mgr()[file_info].file_info_ast.borrow().text_document.as_ref()
        .expect("text_document must be populated after the incremental edit")
        .contents().to_string();
    assert!(contents.starts_with("xclass Thing"), "edit should have applied against the live symbol, got: {contents:?}");
}

#[test]
fn test_delete_open_file_then_close_it_clears_everything() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let dir = env::current_dir().unwrap().join("tests/data/deleted_dir_race").sanitize();
    let point_path = Path::new(&dir).join("point_close.py").sanitize();
    let point_content = std::fs::read_to_string(&point_path).unwrap();
    let point_uri = FileMgr::pathname2uri(&point_path);
    let _restore_fixture = RestoreFixture { path: point_path.clone(), original: point_content.clone() };

    // 1. Open point.py, then actually delete it on disk while it stays open.
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: point_content,
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    std::fs::remove_file(&point_path).unwrap();
    Odoo::handle_did_delete(&mut session, DeleteFilesParams {
        files: vec![FileDelete { uri: point_uri.to_string() }],
    });
    assert!(session.file_mgr().get_file_info(&point_path).is_some(), "must survive the delete while open");

    // 2. The user eventually closes the tab without ever reopening it.
    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: point_uri },
    });

    assert!(
        session.file_mgr().get_file_info(&point_path).is_none(),
        "FileInfo must finally be cleared once closed, not leak for the rest of the session"
    );
    assert!(
        session.sync_odoo.get_symbol(&point_path, (&[], &["Thing"]), u32::MAX).is_empty(),
        "symbol must also be cleared on close, not just its cache"
    );
    assert!(
        !session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter().any(|ep| ep.borrow().path == point_path),
        "entry point must be gone too, not left dangling"
    );
}

// The key regression test: a feature request (hover/completion/etc., all
// resolving via `get_symbol_of_opened_file`) runs on the deleted-but-still-open
// buffer before it's ever saved back. Must resolve the real, live symbol -
// not resurrect a phantom entry.
#[test]
fn test_feature_request_on_deleted_open_file_does_not_resurrect_a_phantom_entry() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let dir = env::current_dir().unwrap().join("tests/data/deleted_dir_race").sanitize();
    let point_path = Path::new(&dir).join("point_phantom.py").sanitize();
    let point_content = std::fs::read_to_string(&point_path).unwrap();
    let point_uri = FileMgr::pathname2uri(&point_path);
    let _restore_fixture = RestoreFixture { path: point_path.clone(), original: point_content.clone() };

    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: point_content,
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    let entries_before = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len();

    std::fs::remove_file(&point_path).unwrap();
    Odoo::handle_did_delete(&mut session, DeleteFilesParams {
        files: vec![FileDelete { uri: point_uri.to_string() }],
    });

    // Simulate hover/completion/goto-definition on the still-open buffer.
    let resolved = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&point_path));
    assert!(resolved.is_some(), "should resolve the real, still-live symbol");
    assert_eq!(
        session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len(), entries_before,
        "no phantom entry point should have been created"
    );

    // Closing now must still fully clear everything, exactly as if the
    // feature request had never happened.
    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: point_uri },
    });
    assert!(
        session.file_mgr().get_file_info(&point_path).is_none(),
        "a feature request must not be able to silently disable cleanup"
    );
}

#[test]
fn test_delete_open_file_then_recreate_it_keeps_it() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let dir = env::current_dir().unwrap().join("tests/data/recreated_file_race").sanitize();
    let point_path = Path::new(&dir).join("point.py").sanitize();
    let point_content = std::fs::read_to_string(&point_path).unwrap();
    let point_uri = FileMgr::pathname2uri(&point_path);
    let _restore_fixture = RestoreFixture { path: point_path.clone(), original: point_content.clone() };

    // 1. Open point.py, then delete it on disk while it stays open.
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: point_content.clone(),
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    Odoo::handle_did_delete(&mut session, DeleteFilesParams {
        files: vec![FileDelete { uri: point_uri.to_string() }],
    });
    assert!(session.file_mgr().get_file_info(&point_path).is_some(), "must survive the delete while open");

    // 2. The user edits the still-open buffer (never saved yet).
    Odoo::handle_did_change(&mut session, DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier { uri: point_uri.clone(), version: 2 },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(1, 8), Position::new(1, 14))),
            range_length: None,
            text: "renamed_method".to_string(),
        }],
    });
    let edited_content = session.file_mgr()[
        session.file_mgr().get_file_info(&point_path).unwrap()
        ].file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();

    // 3. The user saves the buffer, recreating the file on disk.
    std::fs::write(&point_path, &edited_content).unwrap();
    Odoo::handle_did_create(&mut session, CreateFilesParams {
        files: vec![FileCreate { uri: point_uri.to_string() }],
    });
    BuildScheduler::process_rebuilds(&mut session, false);

    assert!(
        session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter().any(|ep| ep.borrow().path == point_path),
        "entry point was never actually removed"
    );
    assert!(
        !session.sync_odoo.get_symbol(&point_path, (&[], &["Thing", "renamed_method"]), u32::MAX).is_empty(),
        "symbol must reflect the edited content, proving didChange kept it fresh throughout"
    );
    assert!(
        session.sync_odoo.get_symbol(&point_path, (&[], &["Thing", "method"]), u32::MAX).is_empty(),
        "the OLD pre-edit method name must be gone, not stale-cached"
    );

    // 4. Closing a legitimately-tracked-again file must keep its FileInfo.
    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: point_uri },
    });
    assert!(session.file_mgr().get_file_info(&point_path).is_some());
}

// Scope boundary, documented rather than silently dropped: deleting a whole
// directory containing an open file is not covered by the per-file rule above
// (that would need the recursive symbol/entry teardown itself to be
// open-aware). This asserts today's unchanged behavior: immediate teardown.
#[test]
fn test_directory_delete_with_nested_open_file_is_not_deferred() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let dir = env::current_dir().unwrap().join("tests/data/nested_dir_race/pkg").sanitize();
    let point_path = Path::new(&dir).join("models").join("point.py").sanitize();
    let point_content = std::fs::read_to_string(&point_path).unwrap();
    let point_uri = FileMgr::pathname2uri(&point_path);
    let pkg_init = Path::new(&dir).join("__init__.py").sanitize();

    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &dir, &pkg_init);
    BuildScheduler::process_rebuilds(&mut session, false);
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri,
            language_id: "python".to_string(),
            version: 1,
            text: point_content,
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    assert!(session.file_mgr()[session.file_mgr().get_file_info(&point_path).unwrap()].opened);
    assert!(!session.sync_odoo.get_symbol(&dir, (&["models", "point"], &["Thing"]), u32::MAX).is_empty());

    // Delete the whole `pkg` dir (not point.py directly). `pkg` has an
    // `__init__.py` (a `PythonPackage`, convertible to a `SourceFileKey`) -
    // unlike the bare `models` subdir, which `unload_path` can't resolve at all.
    Odoo::handle_did_delete(&mut session, DeleteFilesParams {
        files: vec![FileDelete { uri: FileMgr::pathname2uri(&dir).to_string() }],
    });

    assert!(
        session.sync_odoo.get_symbol(&dir, (&["models", "point"], &["Thing"]), u32::MAX).is_empty(),
        "a directory delete tears down nested symbols immediately, even if one is open"
    );
}
