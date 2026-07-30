use lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, GotoDefinitionParams,
    GotoDefinitionResponse, HoverContents, HoverParams, ReferenceContext, ReferenceParams,
};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::threads::SessionInfo;

use super::fixture::FixtureFile;

/// The hover contents at the caret.
pub fn hover(session: &mut SessionInfo, file: &FixtureFile, snippet: &str) -> String {
    let params = HoverParams {
        text_document_position_params: file.position_params(snippet),
        work_done_progress_params: Default::default(),
    };
    let hover = Odoo::handle_hover(session, params)
        .expect("hover request failed")
        .unwrap_or_else(|| panic!("no hover at {snippet:?} in {}", file.path));
    match hover.contents {
        HoverContents::Markup(markup) => markup.value,
        contents => panic!("unexpected hover contents at {snippet:?}: {contents:?}"),
    }
}

/// Definition targets at the caret, as `(path, start line)`.
pub fn definitions(session: &mut SessionInfo, file: &FixtureFile, snippet: &str) -> Vec<(String, u32)> {
    let params = GotoDefinitionParams {
        text_document_position_params: file.position_params(snippet),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let response = Odoo::handle_goto_definition(session, params)
        .expect("definition request failed")
        .unwrap_or_else(|| panic!("no definition at {snippet:?} in {}", file.path));
    let targets = match response {
        GotoDefinitionResponse::Scalar(location) => vec![(location.uri, location.range)],
        GotoDefinitionResponse::Array(locations) => {
            locations.into_iter().map(|l| (l.uri, l.range)).collect()
        }
        GotoDefinitionResponse::Link(links) => {
            links.into_iter().map(|l| (l.target_uri, l.target_range)).collect()
        }
    };
    targets
        .into_iter()
        .map(|(uri, range)| (FileMgr::uri2pathname(uri.as_str()), range.start.line))
        .collect()
}

/// The items offered at the caret.
pub fn completions(session: &mut SessionInfo, file: &FixtureFile, snippet: &str) -> Vec<CompletionItem> {
    let params = CompletionParams {
        text_document_position: file.position_params(snippet),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let response = Odoo::handle_autocomplete(session, params)
        .expect("completion request failed")
        .unwrap_or_else(|| panic!("no completion at {snippet:?} in {}", file.path));
    match response {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    }
}

/// The item labelled `label` at the caret, after the `completionItem/resolve` round trip a client
/// makes on the item it highlights.
pub fn resolved_completion(session: &mut SessionInfo, file: &FixtureFile, snippet: &str, label: &str) -> CompletionItem {
    let item = completions(session, file, snippet).into_iter().find(|item| item.label == label)
        .unwrap_or_else(|| panic!("no completion {label:?} at {snippet:?} in {}", file.path));
    Odoo::handle_completion_resolve(session, item).expect("completion resolve failed")
}



/// Reference locations at the caret, as `(path, start line, start character)`.
/// Two invariants hold of every reference set, so they are checked here instead
/// of case by case: no location may point into an OWL virtual doc or shim,
/// which the client would fail to open, and none may repeat
pub fn references(session: &mut SessionInfo, file: &FixtureFile, snippet: &str) -> Vec<(String, u32, u32)> {
    let params = ReferenceParams {
        text_document_position: file.position_params(snippet),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: ReferenceContext { include_declaration: true },
    };
    let locations = Odoo::handle_references(session, params)
        .expect("references request failed")
        .unwrap_or_else(|| panic!("no references at {snippet:?} in {}", file.path));
    let found: Vec<(String, u32, u32)> = locations.into_iter()
        .map(|l| (FileMgr::uri2pathname(l.uri.as_str()), l.range.start.line, l.range.start.character))
        .collect();
    for (path, ..) in found.iter() {
        assert!(
            !path.contains("__ols_owl__") && !path.contains("__ols_shim__"),
            "references at {snippet:?} leaked the internal path {path:?}"
        );
    }
    let mut deduped = found.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), found.len(), "references at {snippet:?} repeat a location: {found:?}");
    found
}
