use std::cell::RefCell;
use std::rc::Rc;

use lsp_types::{CompletionContext, CompletionResponse, CompletionTriggerKind};
use odoo_ls_server::core::file_mgr::{FileInfo, FileMgr};
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::core::symbols::symbol_keys::SourceFileKey;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::threads::SessionInfo;

use crate::setup::setup::{create_init_session, setup_server};
mod setup;

/// Open an untitled python buffer and return its file symbol + info.
/// Untitled buffers don't need an Odoo install, which keeps these tests
/// focused on the trigger-kind gate and runnable without COMMUNITY_PATH.
fn open_untitled(session: &mut SessionInfo, uri: &str, text: &str) -> (SourceFileKey, Rc<RefCell<FileInfo>>) {
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(uri),
            language_id: "python".to_string(),
            version: 1,
            text: text.to_string(),
        }
    };
    Odoo::handle_did_open(session, did_open_params);
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(session, &std::path::PathBuf::from(uri))
        .expect("Untitled file symbol");
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(uri).unwrap();
    (file_symbol, file_info)
}

fn ctx(trigger_kind: CompletionTriggerKind, trigger_character: Option<&str>) -> CompletionContext {
    CompletionContext {
        trigger_kind,
        trigger_character: trigger_character.map(|c| c.to_string()),
    }
}

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let items = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|i| i.label).collect()
}

/// Regression test for https://github.com/odoo/odoo-ls/issues/518
/// A `,` (trigger character) after an argument must NOT dump every name in scope.
/// This is the exact shape of `fields.Selection([...], )`.
#[test]
fn test_comma_trigger_char_returns_nothing() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    // `foo([('a', 'b')], )` -- cursor just before the closing paren, i.e. in the
    // gap after the trailing comma. char count: foo(=4, list `[('a', 'b')]`=12 -> 16, `,`=17, ` `=18.
    let (fsym, finfo) = open_untitled(&mut session, "untitled:completion-comma", "foo([('a', 'b')], )\n");

    let response = CompletionFeature::autocomplete(
        &mut session, fsym, &finfo,
        Some(ctx(CompletionTriggerKind::TRIGGER_CHARACTER, Some(","))),
        0, 18,
    );
    assert!(response.is_none(), "Comma trigger char should not fall back to name completion, got: {:?}", labels(response));
}

/// The SAME position, when reached by manual invocation (Ctrl+Space), SHOULD still
/// list names in scope. Proves the gate only affects trigger-character events.
#[test]
fn test_comma_position_invoked_still_completes() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let (fsym, finfo) = open_untitled(&mut session, "untitled:completion-invoked", "spam = 1\nfoo(spam, )\n");

    // `foo(spam, )` on line 1: foo(=4, spam=8, `,`=9, ` `=10 -> cursor at 11 (before `)`).
    let response = CompletionFeature::autocomplete(
        &mut session, fsym, &finfo,
        Some(ctx(CompletionTriggerKind::INVOKED, None)),
        1, 11,
    );
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "spam"),
        "Manual invocation should still list names in scope, got: {:?}", labels);
}

/// A missing context (older clients / internal callers) must behave like manual
/// invocation, not like a trigger character. Guards against suppressing valid completions.
#[test]
fn test_missing_context_behaves_like_invoked() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let (fsym, finfo) = open_untitled(&mut session, "untitled:completion-noctx", "spam = 1\nfoo(spam, )\n");

    let response = CompletionFeature::autocomplete(&mut session, fsym, &finfo, None, 1, 11);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "spam"),
        "Missing context should fall back to name completion, got: {:?}", labels);
}

/// The gate must not touch attribute completion: `.` is a trigger character, but
/// attribute completion returns Some from the structured path, so it never reaches
/// the gated fallback.
#[test]
fn test_dot_trigger_char_still_completes() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    // `spam.` -- cursor after the dot on line 1.
    let (fsym, finfo) = open_untitled(&mut session, "untitled:completion-dot", "spam = 1\nspam.\n");

    let response = CompletionFeature::autocomplete(
        &mut session, fsym, &finfo,
        Some(ctx(CompletionTriggerKind::TRIGGER_CHARACTER, Some("."))),
        1, 5,
    );
    assert!(response.is_some(),
        "Attribute completion on `.` must not be suppressed by the trigger-char gate");
}
