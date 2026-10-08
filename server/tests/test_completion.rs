use std::env;
use std::path::Path;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let items = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|i| i.label).collect()
}

#[test]
fn test_completions() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    test_depends_kwarg_nested_field_completion(&mut session);
    test_lambda_is_not_a_member(&mut session);
    test_compute_sql_kwarg_method_completion(&mut session);
    test_selection_field_method_completion(&mut session);
    test_init_storage_kwarg_method_completion(&mut session);
    test_order_completion_with_escape(&mut session);
    test_order_completion_by_word(&mut session);
    test_import_completion_with_normalized_name(&mut session);
}
 
/// `fields.Char(compute="...", depends=["partner_id.disp"])`: the `depends` kwarg should
/// offer nested-field completion on the last dotted segment, exactly like `related=`.
fn test_depends_kwarg_nested_field_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {}", test_file);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Line `    partner_display_name_dep = fields.Char(compute="_compute_partner_display_name_dep", depends=["partner_id.disp"])`
    // (0-indexed line 79), cursor right after "disp" inside the string.
    let response = CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 79, 113);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "display_name"), "Expected display_name to be suggested for the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "create_uid"), "create_uid does not match the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "id"), "id does not match the 'disp' prefix, got: {:?}", labels);
}

/// `default=lambda self: ...` stores a `<lambda>` child on the class, but it is not a member:
/// `self.` must never offer it.
fn test_lambda_is_not_a_member(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let disk_text = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    // Open the buffer with the dot typed after `return self`: the completed attribute name is
    // then empty, so every member of LambdaDefaultModel is offered.
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&test_file),
            language_id: "python".to_string(),
            version: 1,
            text: disk_text.replace("return self\n", "return self.\n"),
        }
    };
    Odoo::handle_did_open(session, did_open_params);

    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // `return self.` is line 25 of the fixture (0-indexed 24), cursor after the dot.
    let response = CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 24, 20);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "company_id"), "Expected the model fields to be suggested after 'self.', got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "<lambda>"), "<lambda> is not a member and must not be suggested, got: {:?}", labels);
}

/// `fields.Integer(compute_sql="...")`: the kwarg offers method completion, but only from 19.1 on
fn test_compute_sql_kwarg_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor inside the compute_sql value of `test_int = fields.Integer(..., compute_sql="...")`
    let initial_version = session.sync_odoo.version;
    session.sync_odoo.version = OdooVersion::new(19, 1, 0);
    let gated_in = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 8, 80));
    assert!(gated_in.iter().any(|l| l == "_compute_something"), "Expected _compute_something to be suggested for compute_sql, got: {:?}", gated_in);
    session.sync_odoo.version = OdooVersion::new(19, 0, 0);
    let gated_out = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 8, 80));
    assert!(!gated_out.iter().any(|l| l == "_compute_something"), "Expected no method completion for compute_sql before 19.1, got: {:?}", gated_out);
    // The session is shared with the other completion tests, leave the version as it was found
    session.sync_odoo.version = initial_version;
}

/// `group_expand=`, `selection=` and the first positional argument of a Selection offer methods
fn test_selection_field_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor after the 4-char prefix of each string on the `state`, `kind` and `label` field lines
    for (line, column, method) in [(85, 34, "_selection_state"), (85, 67, "_expand_states"), (86, 43, "_selection_state")] {
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, line, column));
        assert!(labels.iter().any(|l| l == method), "Expected {method} to be suggested at {line}:{column}, got: {:?}", labels);
    }
    let label_labels = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 87, 29));
    assert!(!label_labels.iter().any(|l| l == "_selection_state"), "The label of a Char is not a method name, got: {:?}", label_labels);
}

/// `init_storage="..."` offers method completion, but only from 20.0 on
fn test_init_storage_kwarg_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    let initial_version = session.sync_odoo.version;
    session.sync_odoo.version = OdooVersion::new(20, 0, 0);
    let gated_in = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 86, 76));
    assert!(gated_in.iter().any(|l| l == "_init_column_kind"), "Expected _init_column_kind to be suggested for init_storage, got: {:?}", gated_in);
    session.sync_odoo.version = OdooVersion::new(19, 4, 0);
    let gated_out = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, 86, 76));
    assert!(!gated_out.iter().any(|l| l == "_init_column_kind"), "Expected no method completion for init_storage before 20.0, got: {:?}", gated_out);
    // The session is shared with the other completion tests, leave the version as it was found
    session.sync_odoo.version = initial_version;
}

/// `_order = "name,\tid"`: the escape makes the decoded value one byte shorter than its source,
/// so a cursor right before the closing quote must not slice past the end of the value.
fn test_order_completion_with_escape(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let content = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    let (line, text) = content.lines().enumerate().find(|(_, text)| text.contains("_order = ")).unwrap();
    let character = text.rfind('"').unwrap() as u32;
    let labels = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, line as u32, character));
    assert!(labels.iter().any(|l| l == "id"), "Expected fields matching the 'id' prefix, got: {:?}", labels);
}

/// `_order = "name desc nulls last, id"`: what is completed depends on the word of the item under
/// the cursor: the field first, then its direction, then where nulls go.
fn test_order_completion_by_word(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let content = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    let (line, text) = content.lines().enumerate().find(|(_, text)| text.contains(r#"_order = "name desc"#)).unwrap();
    let mut failures = vec![];
    // (text right before the cursor, labels that must be offered, labels that must not)
    let cases: [(&str, &[&str], &[&str]); 6] = [
        (r#""na"#, &["name"], &["id", "desc"]),
        (r#""name "#, &["asc", "desc", "nulls first", "nulls last"], &["name", "id"]),
        (r#""name d"#, &["desc"], &["asc", "nulls first", "name"]),
        ("desc ", &["nulls first", "nulls last"], &["asc", "desc", "name"]),
        ("nulls ", &["first", "last"], &["nulls first", "name"]),
        (", ", &["name", "id"], &["asc", "desc"]),
    ];
    for (before_cursor, offered, not_offered) in cases {
        let character = (text.find(before_cursor).unwrap() + before_cursor.len()) as u32;
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, line as u32, character));
        let missing: Vec<_> = offered.iter().filter(|label| !labels.iter().any(|l| l == *label)).collect();
        let unexpected: Vec<_> = not_offered.iter().filter(|label| labels.iter().any(|l| l == *label)).collect();
        if !missing.is_empty() || !unexpected.is_empty() {
            failures.push(format!("after `{before_cursor}`: missing {missing:?}, unexpected {unexpected:?}"));
        }
    }
    assert!(failures.is_empty(), "`_order` completion:\n{}", failures.join("\n"));
}

/// `import ｏ|ｓ`: the AST holds the NFKC-normalized name `os`, so the typed prefix must be read up
/// to the cursor in the source, not by slicing the normalized name with source offsets.
fn test_import_completion_with_normalized_name(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_semantic_tokens").join("models").join("sem_tokens_non_ascii.py").sanitize();

    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor right after `ｏ` (UTF-16 columns) in `import ｏｓ` (0-indexed line 3) and in
    // `from ｏｓ import path` (line 4).
    let mut failures = vec![];
    for (line, character) in [(3, 8), (4, 6)] {
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, &file_info, None, line, character));
        let has_os = labels.iter().any(|l| l == "os");
        let others: Vec<&String> = labels.iter().filter(|l| !l.starts_with('o')).collect();
        if !has_os || !others.is_empty() {
            failures.push(format!("line {line}: `os` suggested: {has_os}, {} names not starting with the typed `o`, e.g. {:?}",
                others.len(), &others[..others.len().min(5)]));
        }
    }
    assert!(failures.is_empty(), "Import completion on an NFKC-normalized name:\n{}", failures.join("\n"));
}
