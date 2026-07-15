use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit, Position, Range, TextEdit};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use crate::core::tsserver_bridge::ts_kind_to_lsp_kind;

/// Round-trip payload stashed in `CompletionItem.data` for auto-import entries, whose
/// import statement only comes back from a `completionEntryDetails` call. tsserver
/// re-locates the entry from the original position + `name`/`source`/`data`, so all of it
/// must survive the trip through the client (`data` is opaque — hand it back untouched).
#[derive(Debug, Serialize, Deserialize)]
pub struct TsCompletionResolveData {
    pub file: String,
    /// 0-based, as originally sent to [`TsServerBridge::completion_items_for_content`].
    pub line: u32,
    pub character: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// The parts of a `completionEntryDetails` response an LSP `completionItem/resolve` can use.
#[derive(Debug, Default)]
pub struct TsCompletionDetails {
    pub detail: Option<String>,
    pub documentation: Option<String>,
    /// Edits in the *requested* file only (an auto-import's `import … from "…";` line).
    pub additional_text_edits: Vec<TextEdit>,
}


pub fn entry_to_completion_item(entry: &Value, file_path: &str, line: u32, character: u32) -> CompletionItem {
    let name = entry.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let insert_text = entry.get("insertText").and_then(Value::as_str);
    let source = entry.get("source").and_then(Value::as_str);

    // tsserver only ever produces single-line replacement spans here; a multi-line one
    // means we misread the response — fall back to `insert_text` rather than corrupt the doc.
    let text_edit = entry
        .get("replacementSpan")
        .and_then(ts_span_to_range)
        .filter(|range| {
            let single_line = range.start.line == range.end.line;
            if !single_line {
                debug!("tsserver: ignoring multi-line replacementSpan for completion {}", name);
            }
            single_line
        })
        .map(|range| CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: insert_text.unwrap_or(&name).to_string(),
        }));

    // Only `hasAction` entries owe a `completionEntryDetails` round trip; asking for the rest
    // would be one blocking tsserver request per item the user scrolls past.
    let data = entry
        .get("hasAction")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        .then(|| serde_json::to_value(TsCompletionResolveData {
            file: file_path.to_string(),
            line,
            character,
            name: name.clone(),
            source: source.map(str::to_string),
            data: entry.get("data").cloned(),
        }).ok())
        .flatten();

    // Where an auto-import would pull the symbol from, shown next to the label.
    let label_details = join_ts_parts(entry.get("sourceDisplay"))
        .or_else(|| source.map(str::to_string))
        .map(|description| CompletionItemLabelDetails {
            detail: None,
            description: Some(description),
        });

    CompletionItem {
        label: name,
        kind: Some(entry
            .get("kind")
            .and_then(Value::as_str)
            .map(ts_kind_to_lsp_kind)
            .unwrap_or(CompletionItemKind::VARIABLE)),
        // tsserver ranks entries itself (locals before auto-import suggestions); without
        // `sort_text` the client falls back to alphabetical and that ordering is lost.
        sort_text: entry.get("sortText").and_then(Value::as_str).map(str::to_string),
        filter_text: ts_filter_text(insert_text),
        insert_text: text_edit.is_none().then(|| insert_text.map(str::to_string)).flatten(),
        text_edit,
        label_details,
        data,
        ..CompletionItem::default()
    }
}

/// Parse a `completionEntryDetails` response into the fields an LSP
/// `completionItem/resolve` can return: the entry's signature (`detail`), its
/// docs, and any auto-import edit. Each code action's description ("Add import
/// from …") is prepended to `detail` as the human-readable hint.
pub fn response_to_completion_details(response: Value, file_path: &str) -> Option<TsCompletionDetails> {
    if !response.get("success").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let details = response.get("body")?.as_array()?.first()?;

    let mut result = TsCompletionDetails {
        detail: join_ts_parts(details.get("displayParts")),
        documentation: join_ts_parts(details.get("documentation")),
        additional_text_edits: vec![],
    };

    let Some(code_actions) = details.get("codeActions").and_then(Value::as_array) else {
        return Some(result);
    };
    for action in code_actions {
        for change in action.get("changes").and_then(Value::as_array).into_iter().flatten() {
            let target = change.get("fileName").and_then(Value::as_str).unwrap_or_default();
            if target != file_path {
                debug!("tsserver: dropping completion edit for another file: {}", target);
                continue;
            }
            for text_change in change.get("textChanges").and_then(Value::as_array).into_iter().flatten() {
                let Some(edit) = ts_text_change_to_edit(text_change) else { continue };
                result.additional_text_edits.push(edit);
            }
        }
    }
    // The action description ("Add import from …") is the only hint of what the extra edit
    // will do, and it reads better than the bare signature.
    let descriptions: Vec<&str> = code_actions
        .iter()
        .filter_map(|a| a.get("description").and_then(Value::as_str))
        .collect();
    if !descriptions.is_empty() {
        result.detail = Some(match result.detail {
            Some(detail) => format!("{}\n{}", descriptions.join("\n"), detail),
            None => descriptions.join("\n"),
        });
    }
    Some(result)
}

/// tsserver returns display strings and docs as `[{ text, kind }]` part lists.
pub fn join_ts_parts(parts: Option<&Value>) -> Option<String> {
    let joined: String = parts?
        .as_array()?
        .iter()
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    (!joined.is_empty()).then_some(joined)
}

/// A tsserver `{ line, offset }` (both 1-based) as an LSP `Position` (both 0-based).
fn ts_pos_to_position(pos: &Value) -> Option<Position> {
    Some(Position::new(
        (pos.get("line").and_then(Value::as_u64)? as u32).saturating_sub(1),
        (pos.get("offset").and_then(Value::as_u64)? as u32).saturating_sub(1),
    ))
}

fn ts_span_to_range(span: &Value) -> Option<Range> {
    Some(Range::new(
        ts_pos_to_position(span.get("start")?)?,
        ts_pos_to_position(span.get("end")?)?,
    ))
}

fn ts_text_change_to_edit(change: &Value) -> Option<TextEdit> {
    Some(TextEdit {
        range: ts_span_to_range(change)?,
        new_text: change.get("newText").and_then(Value::as_str)?.to_string(),
    })
}


/// What a client should match the user's typing against, mirroring VS Code's TS extension:
/// import-statement completions filter on their full insert text (their edit range starts
/// at column 0), `this.` inserts keep the label, bracket accessors filter as `.foo-bar`.
fn ts_filter_text(insert_text: Option<&str>) -> Option<String> {
    let insert_text = insert_text?;
    if insert_text.starts_with("this.") {
        return None;
    }
    if let Some(inner) = insert_text.strip_prefix('[') {
        let inner = inner.strip_suffix(']')?;
        let inner = inner.strip_prefix(['\'', '"'])?.strip_suffix(['\'', '"'])?;
        return Some(format!(".{inner}"));
    }
    Some(insert_text.to_string())
}


#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Verbatim tsserver payload for `import { Domai|` (probed against `@web/core/domain`).
    fn import_statement_entry() -> Value {
        json!({
            "name": "Domain",
            "kind": "class",
            "kindModifiers": "export",
            "sortText": "11",
            "source": "@web/core/domain",
            "insertText": "import { Domain } from \"@web/core/domain\";",
            "replacementSpan": { "start": { "line": 1, "offset": 1 }, "end": { "line": 1, "offset": 15 } },
            "sourceDisplay": [{ "text": "@web/core/domain", "kind": "text" }],
            "isImportStatementCompletion": true,
            "data": { "exportName": "Domain", "fileName": "/odoo/addons/web/static/src/core/domain.js" }
        })
    }

    /// Verbatim tsserver payload for a bare `Domai|` in a code body: no `insertText`, but an
    /// action carrying the import edit.
    fn auto_import_entry() -> Value {
        json!({
            "name": "Domain",
            "kind": "class",
            "kindModifiers": "export",
            "sortText": "16",
            "source": "/odoo/addons/web/static/src/core/domain",
            "hasAction": true,
            "data": { "exportName": "Domain", "fileName": "/odoo/addons/web/static/src/core/domain.js" }
        })
    }

    #[test]
    fn filter_text_mirrors_vscode_rules() {
        // An import statement must filter on itself: its edit range starts at column 0, so the
        // client matches the label against everything the user typed on the line.
        assert_eq!(
            ts_filter_text(Some("import { Domain } from \"@web/core/domain\";")).as_deref(),
            Some("import { Domain } from \"@web/core/domain\";")
        );
        // `this.` inserts filter on the member name the user is actually typing.
        assert_eq!(ts_filter_text(Some("this.rowCount")), None);
        // Bracket accessors replace the typed `.`.
        assert_eq!(ts_filter_text(Some("[\"foo-bar\"]")).as_deref(), Some(".foo-bar"));
        assert_eq!(ts_filter_text(Some("['foo-bar']")).as_deref(), Some(".foo-bar"));
        assert_eq!(ts_filter_text(None), None);
    }

    #[test]
    fn import_statement_entry_becomes_a_text_edit() {
        let item = entry_to_completion_item(&import_statement_entry(), "/a/b.js", 0, 14);

        assert_eq!(item.label, "Domain");
        assert_eq!(item.kind, Some(CompletionItemKind::CLASS));
        assert_eq!(item.sort_text.as_deref(), Some("11"));

        // 1-based tsserver span -> 0-based LSP range covering `import { Domai`.
        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected a text edit built from replacementSpan");
        };
        assert_eq!(edit.range, Range::new(Position::new(0, 0), Position::new(0, 14)));
        assert_eq!(edit.new_text, "import { Domain } from \"@web/core/domain\";");

        // `insert_text` would be applied at the cursor *in addition* to nothing, duplicating
        // the statement fragment; a text edit supersedes it.
        assert_eq!(item.insert_text, None);
        assert_eq!(item.filter_text.as_deref(), Some("import { Domain } from \"@web/core/domain\";"));
        // The whole statement is in the edit, so there is nothing left to resolve.
        assert!(item.data.is_none());
        assert_eq!(
            item.label_details.and_then(|d| d.description).as_deref(),
            Some("@web/core/domain")
        );
    }

    #[test]
    fn auto_import_entry_carries_resolve_data() {
        let item = entry_to_completion_item(&auto_import_entry(), "/a/b.js", 23, 5);

        assert!(item.text_edit.is_none(), "no replacementSpan, so no edit");
        assert!(item.insert_text.is_none());
        assert_eq!(item.sort_text.as_deref(), Some("16"));

        let resolve: TsCompletionResolveData =
            serde_json::from_value(item.data.expect("hasAction entries must be resolvable")).unwrap();
        assert_eq!(resolve.file, "/a/b.js");
        assert_eq!((resolve.line, resolve.character), (23, 5));
        assert_eq!(resolve.name, "Domain");
        assert_eq!(resolve.source.as_deref(), Some("/odoo/addons/web/static/src/core/domain"));
        // tsserver's opaque export-map handle must survive the trip through the client.
        assert_eq!(resolve.data.unwrap()["exportName"], "Domain");

        // Without `sourceDisplay`, the raw `source` names the module in the list.
        assert_eq!(
            item.label_details.and_then(|d| d.description).as_deref(),
            Some("/odoo/addons/web/static/src/core/domain")
        );
    }

    #[test]
    fn plain_member_entry_needs_neither_edit_nor_resolve() {
        let entry = json!({ "name": "rowCount", "kind": "getter", "sortText": "11" });
        let item = entry_to_completion_item(&entry, "/a/b.js", 3, 9);

        assert_eq!(item.kind, Some(CompletionItemKind::PROPERTY));
        assert!(item.text_edit.is_none());
        assert!(item.data.is_none(), "resolve is one blocking round trip; only auto-imports earn it");
        assert!(item.filter_text.is_none());
        assert!(item.label_details.is_none());
    }

    #[test]
    fn multi_line_replacement_span_is_refused() {
        // Clients reject completion ranges spanning lines; tsserver never emits one here, so
        // treat it as a misread response rather than corrupt the document.
        let mut entry = import_statement_entry();
        entry["replacementSpan"]["end"] = json!({ "line": 2, "offset": 3 });

        let item = entry_to_completion_item(&entry, "/a/b.js", 0, 14);
        assert!(item.text_edit.is_none());
    }

    #[test]
    fn text_change_converts_to_a_zero_based_edit() {
        let change = json!({
            "start": { "line": 4, "offset": 1 },
            "end": { "line": 4, "offset": 1 },
            "newText": "import { Domain } from \"@web/core/domain\";\n"
        });
        let edit = ts_text_change_to_edit(&change).unwrap();
        assert_eq!(edit.range, Range::new(Position::new(3, 0), Position::new(3, 0)));
        assert_eq!(edit.new_text, "import { Domain } from \"@web/core/domain\";\n");
    }

    #[test]
    fn ts_parts_join_into_one_string() {
        let parts = json!([{ "text": "class ", "kind": "keyword" }, { "text": "Domain", "kind": "className" }]);
        assert_eq!(join_ts_parts(Some(&parts)).as_deref(), Some("class Domain"));
        assert_eq!(join_ts_parts(Some(&json!([]))), None);
        assert_eq!(join_ts_parts(None), None);
    }
}
