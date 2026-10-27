use assert_fs::TempDir;
use assert_fs::prelude::*;
use lsp_types::{DiagnosticSeverity, NumberOrString, TextDocumentContentChangeEvent, Position};
use odoo_ls_server::constants::DiagnosticSource;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::core::tsserver_bridge::{TsDiagnostic, Utf16Position};
use odoo_ls_server::threads::TsServerDiagnostics;
use odoo_ls_server::utils::PathSanitizer;
use ruff_source_file::PositionEncoding;

use crate::setup::setup::{create_init_session, get_diagnostics_for_path, setup_server};

mod setup;

#[test]
/// Simple test to verify that we can handle unicode characters in files.
/// We add 2 emojis, each has 2 UTF-16 unicode code units, and we ensure that
/// all operations (insert, delete, insert at end) work correctly without panicking.
fn test_unicode_file_lifecycle() {
    // Setup server and session
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let unicode_uri = format!("{}", std::env::current_dir().unwrap().join("data").join("test_unicode.py").display());
    let initial_text =
"def func() -> int:
    return 42
x = func()
";
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&unicode_uri),
            language_id: "python".to_string(),
            version: 1,
            text: initial_text.to_string(),
        }
    };
    odoo_ls_server::core::odoo::Odoo::handle_did_open(&mut session, did_open_params);
    // Add new line and two emojis
    let change_event = TextDocumentContentChangeEvent {
        range: Some(lsp_types::Range {
            start: Position::new(2, 10),
            end: Position::new(2, 10),
        }),
        range_length: None,
        text: "
😊😊".to_string(),
    };
    let did_change_params = lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier {
            uri: FileMgr::pathname2uri(&unicode_uri),
            version: 2,
        },
        content_changes: vec![change_event],
    };
    odoo_ls_server::core::odoo::Odoo::handle_did_change(&mut session, did_change_params);
    // Delete one emoji
    let change_event = TextDocumentContentChangeEvent {
        range: Some(lsp_types::Range {
            start: Position::new(3, 2),
            end: Position::new(3, 4),
        }),
        range_length: None,
        text: "".to_string(),
    };
    let did_change_params = lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier {
            uri: FileMgr::pathname2uri(&unicode_uri),
            version: 3,
        },
        content_changes: vec![change_event],
    };
    odoo_ls_server::core::odoo::Odoo::handle_did_change(&mut session, did_change_params);
    // Attempt to add one character at the end now
    let change_event = TextDocumentContentChangeEvent {
        range: Some(lsp_types::Range {
            start: Position::new(3, 2),
            end: Position::new(3, 2),
        }),
        range_length: None,
        text: "c".to_string(),
    };
    let did_change_params = lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier {
            uri: FileMgr::pathname2uri(&unicode_uri),
            version: 4,
        },
        content_changes: vec![change_event],
    };
    odoo_ls_server::core::odoo::Odoo::handle_did_change(&mut session, did_change_params);
    // If the test does not panic, we are good
    // Using ropey, this would've panic
}

/// tsserver positions are UTF-16 whatever encoding the client negotiated.
#[test]
fn test_tsserver_diagnostic_position_encoding() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    session.sync_odoo.encoding = PositionEncoding::Utf8;

    let temp_dir = TempDir::new().unwrap();
    let js_file = temp_dir.child("emoji.js");
    let content = "const s = \"😊\"; bad;\n";
    js_file.write_str(content).unwrap();
    let js_path = js_file.path().sanitize();
    Odoo::handle_did_open(&mut session, lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&js_path),
            language_id: "javascript".to_string(),
            version: 1,
            text: content.to_string(),
        },
    });

    // `bad` is at UTF-16 columns 16..19, UTF-8 columns 18..21
    Odoo::handle_tsserver_new_diagnostics(&mut session, TsServerDiagnostics {
        file: js_path.clone(),
        diagnostic_level: DiagnosticSource::JS_TSSERVER_SEMANTIC,
        diagnostics: vec![TsDiagnostic {
            range: Utf16Position { line: 0, character: 16 }..Utf16Position { line: 0, character: 19 },
            severity: DiagnosticSeverity::ERROR,
            code: "tsserver_2304".to_string(),
            message: "Cannot find name 'bad'.".to_string(),
        }],
    });

    let diagnostics = get_diagnostics_for_path(&mut session, &js_path);
    let tsserver_diag = diagnostics.iter()
        .find(|d| d.code == Some(NumberOrString::String("tsserver_2304".to_string())))
        .expect("the tsserver diagnostic must be published");
    assert_eq!(
        tsserver_diag.range,
        lsp_types::Range { start: Position::new(0, 18), end: Position::new(0, 21) },
    );
}