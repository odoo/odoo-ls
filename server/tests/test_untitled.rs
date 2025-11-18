use lsp_types::{TextDocumentContentChangeEvent, Position};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::features::hover::HoverFeature;
use odoo_ls_server::features::definition::DefinitionFeature;
mod setup;

#[test]
/// Simple test to verify LSP features on untitled files, open, change, hover, complete.
fn test_untitled_file_lifecycle() {
    // Setup server and session
    let mut odoo = setup::setup::setup_server(false);
    let mut session = setup::setup::create_session(&mut odoo);
    let untitled_uri = "untitled:Untitled-1".to_string();
    let initial_text = "def foo():\n    return 42\nfoo()\n";

    // Simulate didOpen for untitled file
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&untitled_uri),
            language_id: "python".to_string(),
            version: 1,
            text: initial_text.to_string(),
        }
    };
    odoo_ls_server::core::odoo::Odoo::handle_did_open(&mut session, did_open_params);

    // Simulate didChange for untitled file
    let change_event2 = TextDocumentContentChangeEvent {
        range: Some(lsp_types::Range {
            start: Position::new(1, 11),
            end: Position::new(1, 13),
        }),
        range_length: Some(2),
        text: "43".to_string(),
    };
    let did_change_params = lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier {
            uri: FileMgr::pathname2uri(&untitled_uri),
            version: 2,
        },
        content_changes: vec![change_event2],
    };
    odoo_ls_server::core::odoo::Odoo::handle_did_change(&mut session, did_change_params);

    // Get file symbol
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, &std::path::PathBuf::from(&untitled_uri)).expect("Untitled file symbol");
    // Hover on foo()
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&untitled_uri).unwrap();
    let hover = HoverFeature::hover_python(&mut session, &file_symbol, &file_info, 2, 0);
    assert!(hover.is_some(), "Hover result should be Some");
    let hover_content = match hover {
        Some(lsp_types::Hover { contents, .. }) => {
            match contents {
                lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(s)) => s,
                lsp_types::HoverContents::Scalar(lsp_types::MarkedString::LanguageString(ls)) => ls.value,
                lsp_types::HoverContents::Array(arr) => arr.iter().map(|ms| match ms {
                    lsp_types::MarkedString::String(s) => s.clone(),
                    lsp_types::MarkedString::LanguageString(ls) => ls.value.clone(),
                }).collect::<Vec<_>>().join("\n"),
                lsp_types::HoverContents::Markup(markup) => markup.value,
            }
        },
        None => String::new(),
    };
    assert!(hover_content.contains("foo"), "Hover should contain function name 'foo', got: {}", hover_content);
    assert!(hover_content.contains("def foo()"), "Hover should show function signature, got: {}", hover_content);

    // Completion at return
    let completion = CompletionFeature::autocomplete(&mut session, &file_symbol, &file_info, 1, 11);
    assert!(completion.is_some(), "Completion result should be Some");
    let completion_items = match completion.unwrap() {
        lsp_types::CompletionResponse::Array(items) => items,
        lsp_types::CompletionResponse::List(list) => list.items,
    };
    assert!(!completion_items.is_empty(), "Completion items should not be empty");
    let labels: Vec<_> = completion_items.iter().map(|item| item.label.clone()).collect();
    assert!(labels.iter().any(|l| l.contains("foo")), "Completion should contain 'foo', got: {:?}", labels);

    // Definition for foo
    let definition = DefinitionFeature::get_location(&mut session, &file_symbol, &file_info, 0, 4);
    assert!(definition.is_some(), "Definition result should be Some");
    let def_locs = match definition.unwrap() {
        lsp_types::GotoDefinitionResponse::Scalar(loc) => vec![loc],
        lsp_types::GotoDefinitionResponse::Array(arr) => arr,
        lsp_types::GotoDefinitionResponse::Link(arr) => arr.into_iter().map(|l| {
            // Convert LocationLink to Location using target_uri and target_range
            lsp_types::Location {
                uri: l.target_uri,
                range: l.target_range,
            }
        }).collect(),
    };
    assert!(!def_locs.is_empty(), "Definition locations should not be empty");
    let def_loc = &def_locs[0];
    // Should point to line 0 (def foo)
    assert_eq!(def_loc.range.start.line, 0, "Definition should start at line 0 (function definition), got: {:?}", def_loc);
    assert_eq!(def_loc.range.end.line, 1, "Definition should end at line 0 (function definition), got: {:?}", def_loc);
}
