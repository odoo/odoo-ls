use lsp_types::{
    CompletionItem, CompletionItemKind,
};
use odoo_ls_server::threads::SessionInfo;

use crate::js_owl_helpers::requests::*;

use super::fixture::FixtureFile;

/// Assert the hover at the caret contains `expected`. Only the `name: type` half of tsserver's
/// output is worth asserting: the `(property)` / `(getter)` prefix moves between TS versions.
pub fn assert_hover(session: &mut SessionInfo, file: &FixtureFile, snippet: &str, expected: &str) {
    let hover = hover(session, file, snippet);
    assert!(
        hover.contains(expected),
        "hover at {snippet:?} is {hover:?}, expected it to contain {expected:?}"
    );
}

/// Assert the definition at the caret lands in `target`, on the line holding `target_snippet`.
pub fn assert_definition(session: &mut SessionInfo, file: &FixtureFile, snippet: &str, target: &FixtureFile, target_snippet: &str) {
    let expected = (target.path.clone(), target.caret(target_snippet).line);
    let found = definitions(session, file, snippet);
    assert!(
        found.contains(&expected),
        "definition of {snippet:?} is {found:?}, expected {expected:?} ({target_snippet:?})"
    );
}

/// Assert the definition at the caret lands in a file whose path ends with `path_suffix`.
pub fn assert_definition_in(session: &mut SessionInfo, file: &FixtureFile, snippet: &str, path_suffix: &str) {
    let found = definitions(session, file, snippet);
    assert!(
        found.iter().any(|(path, _)| path.ends_with(path_suffix)),
        "definition of {snippet:?} is {found:?}, expected a target in {path_suffix:?}"
    );
}

/// Assert one completion request at the caret offers every `(label, kind)`. The kind matters as
/// much as the label: clients render `TEXT` as a plain word suggestion, so a symbol whose kind
/// went unmapped looks like editor noise.
pub fn assert_completions(session: &mut SessionInfo, file: &FixtureFile, snippet: &str, expected: &[(&str, CompletionItemKind)]) {
    let items = completions(session, file, snippet);
    for (label, kind) in expected {
        let item = items.iter().find(|item| item.label == *label).unwrap_or_else(|| panic!(
            "no completion {label:?} at {snippet:?} in {}", file.path
        ));
        assert_eq!(item.kind, Some(*kind), "kind of completion {label:?} at {snippet:?}");
    }
}

/// Assert the references at the caret are exactly `expected`, each given as a caret snippet in the
/// fixture holding it.
pub fn assert_references(session: &mut SessionInfo, file: &FixtureFile, snippet: &str, expected: &[(&FixtureFile, &str)]) {
    let mut expected: Vec<(String, u32, u32)> = expected.iter().map(|(target, target_snippet)| {
        let position = target.caret(target_snippet);
        (target.path.clone(), position.line, position.character)
    }).collect();
    expected.sort();
    let mut found = references(session, file, snippet);
    found.sort();
    assert_eq!(found, expected, "references of {snippet:?} in {}", file.path);
}

/// Assert the `detail` field of a completion item contains `expected`.
pub fn assert_detail(item: &CompletionItem, expected: &str) {
    let detail = item.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains(expected),
        "detail of completion {:?} is {detail:?}, expected it to contain {expected:?}",
        item.label
    );
}
