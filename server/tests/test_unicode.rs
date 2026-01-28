use lsp_types::{TextDocumentContentChangeEvent, Position};
use odoo_ls_server::core::file_mgr::FileMgr;

use crate::setup::setup::{create_init_session, setup_server};

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