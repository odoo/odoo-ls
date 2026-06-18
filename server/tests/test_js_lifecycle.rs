use std::fs;
use std::path::PathBuf;

use lsp_types::TextDocumentContentChangeEvent;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

fn make_js_open_params(uri: lsp_types::Uri, content: &str) -> lsp_types::DidOpenTextDocumentParams {
    lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri,
            language_id: "javascript".to_string(),
            version: 1,
            text: content.to_string(),
        },
    }
}

fn make_js_change_params(uri: lsp_types::Uri, version: i32, content: &str) -> lsp_types::DidChangeTextDocumentParams {
    lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri, version },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: content.to_string(),
        }],
    }
}

fn make_js_close_params(uri: lsp_types::Uri) -> lsp_types::DidCloseTextDocumentParams {
    lsp_types::DidCloseTextDocumentParams {
        text_document: lsp_types::TextDocumentIdentifier { uri },
    }
}

/// Full JS file lifecycle: open → edit → close → rename → reopen.
/// Uses with_odoo=true to match the expected runtime environment for JS files.
#[test]
fn test_js_file_lifecycle_with_odoo() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let temp_dir = std::env::temp_dir().join(format!("odoo_ls_js_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let js_file = temp_dir.join("test_component.js");
    let initial_content = "/** @odoo-module */\nexport class TestComponent {}\n";
    fs::write(&js_file, initial_content).expect("Failed to write JS file");

    let js_path = js_file.sanitize();
    let js_uri = FileMgr::pathname2uri(&js_path);

    // — open —
    odoo_ls_server::core::odoo::Odoo::handle_did_open(
        &mut session,
        make_js_open_params(js_uri.clone(), initial_content),
    );

    assert!(
        session.sync_odoo.opened_files.contains(&js_path),
        "JS file should be in opened_files after didOpen"
    );
    assert!(
        session.sync_odoo.get_file_mgr().borrow().get_file_info(&js_path).is_some(),
        "FileInfo should exist after didOpen"
    );
    let has_custom_entry = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path.contains("test_component"));
    assert!(has_custom_entry, "Custom entry point should be created for the JS file");

    // — edit —
    let edited_content = "/** @odoo-module */\nexport class TestComponent { setup() {} }\n";
    odoo_ls_server::core::odoo::Odoo::handle_did_change(
        &mut session,
        make_js_change_params(js_uri.clone(), 2, edited_content),
    );

    assert!(
        session.sync_odoo.opened_files.contains(&js_path),
        "JS file should still be in opened_files after didChange"
    );
    {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let file_mgr = file_mgr.borrow();
        let file_info = file_mgr.get_file_info(&js_path).expect("FileInfo must exist after edit");
        assert_eq!(
            file_info.borrow().version,
            Some(2),
            "File version should be updated to 2 after didChange"
        );
    }

    // — close —
    odoo_ls_server::core::odoo::Odoo::handle_did_close(
        &mut session,
        make_js_close_params(js_uri.clone()),
    );

    assert!(
        !session.sync_odoo.opened_files.contains(&js_path),
        "JS file should be removed from opened_files after didClose"
    );
    let entry_after_close = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path.contains("test_component") && !ep.borrow().path.contains("renamed"));
    assert!(!entry_after_close, "Custom entry for original JS file should be removed after didClose");

    // — rename on disk —
    let new_js_file = temp_dir.join("test_component_renamed.js");
    fs::rename(&js_file, &new_js_file).expect("Failed to rename JS file on disk");

    let new_js_path = new_js_file.sanitize();
    let new_js_uri = FileMgr::pathname2uri(&new_js_path);

    odoo_ls_server::core::odoo::Odoo::handle_did_rename(
        &mut session,
        lsp_types::RenameFilesParams {
            files: vec![lsp_types::FileRename {
                old_uri: js_uri.to_string(),
                new_uri: new_js_uri.to_string(),
            }],
        },
    );

    assert!(
        !session.sync_odoo.opened_files.contains(&js_path),
        "Old path should not be in opened_files after rename"
    );

    // — open renamed file —
    odoo_ls_server::core::odoo::Odoo::handle_did_open(
        &mut session,
        make_js_open_params(new_js_uri.clone(), edited_content),
    );

    assert!(
        session.sync_odoo.opened_files.contains(&new_js_path),
        "Renamed JS file should be in opened_files after re-open"
    );
    assert!(
        session.sync_odoo.get_file_mgr().borrow().get_file_info(&new_js_path).is_some(),
        "FileInfo should exist for renamed JS file"
    );
    let has_renamed_entry = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path.contains("test_component_renamed"));
    assert!(has_renamed_entry, "Custom entry point should exist for renamed JS file");

    // — edit renamed file —
    let final_content = "/** @odoo-module */\nexport class TestComponent { setup() {} destroy() {} }\n";
    odoo_ls_server::core::odoo::Odoo::handle_did_change(
        &mut session,
        make_js_change_params(new_js_uri.clone(), 2, final_content),
    );
    {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let file_mgr = file_mgr.borrow();
        let file_info = file_mgr.get_file_info(&new_js_path).expect("FileInfo must exist after edit of renamed file");
        assert_eq!(
            file_info.borrow().version,
            Some(2),
            "Renamed file version should be 2 after edit"
        );
    }

    // — close renamed file —
    odoo_ls_server::core::odoo::Odoo::handle_did_close(
        &mut session,
        make_js_close_params(new_js_uri.clone()),
    );

    assert!(
        !session.sync_odoo.opened_files.contains(&new_js_path),
        "Renamed JS file should be removed from opened_files after final close"
    );

    fs::remove_dir_all(&temp_dir).ok();
}

/// A JS file inside a static/lib/ path should not get OXC diagnostics.
/// Ensures the is_lib exclusion introduced in the last commit works end-to-end.
#[test]
fn test_js_lib_file_no_oxc_diagnostics() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let temp_dir = std::env::temp_dir().join(format!("odoo_ls_js_lib_test_{}", std::process::id()));
    let lib_dir = temp_dir.join("static").join("lib");
    fs::create_dir_all(&lib_dir).expect("Failed to create static/lib dir");

    let lib_js = lib_dir.join("external_lib.js");
    // Deliberately invalid JS — OXC would flag it if not excluded
    let bad_js = "this is not valid javascript @@@;\n";
    fs::write(&lib_js, bad_js).expect("Failed to write lib JS");

    let lib_path = lib_js.sanitize();
    let lib_uri = FileMgr::pathname2uri(&lib_path);

    odoo_ls_server::core::odoo::Odoo::handle_did_open(
        &mut session,
        make_js_open_params(lib_uri.clone(), bad_js),
    );

    let diagnostics = setup::setup::get_diagnostics_for_path(&mut session, &lib_path);
    let oxc_diagnostics: Vec<_> = diagnostics.iter().filter(|d| {
        matches!(&d.source, Some(s) if s.contains("oxc") || s.contains("OXC"))
    }).collect();
    assert!(
        oxc_diagnostics.is_empty(),
        "Files under static/lib/ should not receive OXC diagnostics, got: {:?}",
        oxc_diagnostics
    );

    odoo_ls_server::core::odoo::Odoo::handle_did_close(
        &mut session,
        make_js_close_params(lib_uri),
    );

    fs::remove_dir_all(&temp_dir).ok();
}
