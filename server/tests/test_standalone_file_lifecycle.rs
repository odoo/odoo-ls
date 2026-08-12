use std::fs;

use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

fn make_py_open_params(uri: lsp_types::Uri, content: &str) -> lsp_types::DidOpenTextDocumentParams {
    lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri,
            language_id: "python".to_string(),
            version: 1,
            text: content.to_string(),
        },
    }
}

fn make_py_close_params(uri: lsp_types::Uri) -> lsp_types::DidCloseTextDocumentParams {
    lsp_types::DidCloseTextDocumentParams {
        text_document: lsp_types::TextDocumentIdentifier { uri },
    }
}

fn has_custom_entry(session: &odoo_ls_server::threads::SessionInfo, needle: &str) -> bool {
    session.sync_odoo.entry_point_mgr.custom_entry_points.iter()
        .any(|&ep| session.ep_mgr()[ep].path.contains(needle))
}

/// Standalone (non-module) Python file: `did_create` must not eagerly build it;
/// `did_open`/`did_close` remain the source of truth for the custom entry point.
#[test]
fn test_standalone_python_create_open_close_lifecycle() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let temp_dir = std::env::temp_dir().join(format!("odoo_ls_standalone_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let py_file = temp_dir.join("standalone_script.py");
    let content = "class Foo:\n    pass\n\nfoo = Foo()\n";
    fs::write(&py_file, content).expect("Failed to write python file");

    let py_path = py_file.sanitize();
    let py_uri = FileMgr::pathname2uri(&py_path);

    // — create —
    Odoo::handle_did_create(
        &mut session,
        lsp_types::CreateFilesParams {
            files: vec![lsp_types::FileCreate { uri: py_uri.to_string() }],
        },
    );

    assert!(
        !has_custom_entry(&session, "standalone_script"),
        "did_create must not eagerly create a custom entry point"
    );

    // — open —
    Odoo::handle_did_open(&mut session, make_py_open_params(py_uri.clone(), content));

    assert!(
        has_custom_entry(&session, "standalone_script"),
        "did_open should create the custom entry point"
    );
    let file_info = session.file_mgr().get_file_info(&py_path)
        .expect("FileInfo should exist after open");
    assert!(
        session.file_mgr()[file_info].file_info_ast.borrow().ast.is_built(),
        "ast should be built after open"
    );

    // — close —
    Odoo::handle_did_close(&mut session, make_py_close_params(py_uri.clone()));

    assert!(
        !has_custom_entry(&session, "standalone_script"),
        "did_close should remove the custom entry point"
    );

    fs::remove_dir_all(&temp_dir).ok();
}

/// Renaming a standalone file must not eagerly (re)create its entry point either —
/// only a subsequent did_open (or a lazy feature-request fallback) should.
#[test]
fn test_standalone_python_rename_defers_entry_to_reopen() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let temp_dir = std::env::temp_dir().join(format!("odoo_ls_standalone_rename_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let py_file = temp_dir.join("rename_me.py");
    let content = "value = 1\n";
    fs::write(&py_file, content).expect("Failed to write python file");

    let py_path = py_file.sanitize();
    let py_uri = FileMgr::pathname2uri(&py_path);

    Odoo::handle_did_open(&mut session, make_py_open_params(py_uri.clone(), content));
    assert!(has_custom_entry(&session, "rename_me"), "entry should exist while open");

    Odoo::handle_did_close(&mut session, make_py_close_params(py_uri.clone()));

    let new_py_file = temp_dir.join("renamed.py");
    fs::rename(&py_file, &new_py_file).expect("Failed to rename file on disk");
    let new_py_path = new_py_file.sanitize();
    let new_py_uri = FileMgr::pathname2uri(&new_py_path);

    Odoo::handle_did_rename(
        &mut session,
        lsp_types::RenameFilesParams {
            files: vec![lsp_types::FileRename {
                old_uri: py_uri.to_string(),
                new_uri: new_py_uri.to_string(),
            }],
        },
    );

    assert!(
        !has_custom_entry(&session, "renamed"),
        "did_rename must not eagerly create a custom entry point for the new path"
    );

    Odoo::handle_did_open(&mut session, make_py_open_params(new_py_uri.clone(), content));

    assert!(
        has_custom_entry(&session, "renamed"),
        "did_open on the renamed path should create the entry"
    );

    fs::remove_dir_all(&temp_dir).ok();
}

/// A file created but never opened has no entry point (per the fix above), so any
/// feature request against it must lazily build it on demand instead of panicking.
#[test]
fn test_hover_on_unopened_standalone_file_builds_lazily() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let temp_dir = std::env::temp_dir().join(format!("odoo_ls_standalone_hover_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let py_file = temp_dir.join("never_opened.py");
    let content = "class Foo:\n    pass\n\nfoo = Foo()\n";
    fs::write(&py_file, content).expect("Failed to write python file");

    let py_path = py_file.sanitize();
    let py_uri = FileMgr::pathname2uri(&py_path);

    Odoo::handle_did_create(
        &mut session,
        lsp_types::CreateFilesParams {
            files: vec![lsp_types::FileCreate { uri: py_uri.to_string() }],
        },
    );
    assert!(
        !has_custom_entry(&session, "never_opened"),
        "sanity check: did_create should not have created an entry"
    );

    // Hover on "foo" in "foo = Foo()" (line 3), without ever sending did_open.
    let result = Odoo::handle_hover(
        &mut session,
        lsp_types::HoverParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: py_uri.clone() },
                position: lsp_types::Position { line: 3, character: 1 },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        },
    );

    assert!(
        result.is_ok(),
        "hover on an unopened standalone file must not error/panic: {:?}",
        result.err()
    );
    assert!(
        has_custom_entry(&session, "never_opened"),
        "hover should lazily create the custom entry point on demand"
    );
    let file_info = session.file_mgr().get_file_info(&py_path)
        .expect("FileInfo should exist after hover");
    assert!(
        session.file_mgr()[file_info].file_info_ast.borrow().ast.is_built(),
        "ast should be built lazily by the hover request"
    );

    fs::remove_dir_all(&temp_dir).ok();
}

/// Deleting a standalone file that was created but never opened (so it never got an
/// entry point) must be a clean no-op, not a panic on missing state.
#[test]
fn test_did_delete_on_never_opened_standalone_file_is_a_noop() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let temp_dir = std::env::temp_dir().join(format!("odoo_ls_standalone_delete_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let py_file = temp_dir.join("create_then_delete.py");
    fs::write(&py_file, "value = 1\n").expect("Failed to write python file");

    let py_path = py_file.sanitize();
    let py_uri = FileMgr::pathname2uri(&py_path);

    Odoo::handle_did_create(
        &mut session,
        lsp_types::CreateFilesParams {
            files: vec![lsp_types::FileCreate { uri: py_uri.to_string() }],
        },
    );

    fs::remove_file(&py_file).ok();
    Odoo::handle_did_delete(
        &mut session,
        lsp_types::DeleteFilesParams {
            files: vec![lsp_types::FileDelete { uri: py_uri.to_string() }],
        },
    );

    assert!(
        !has_custom_entry(&session, "create_then_delete"),
        "no entry should exist after create+delete without ever opening"
    );

    fs::remove_dir_all(&temp_dir).ok();
}
