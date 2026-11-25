use std::env;
use std::path::Path;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let items = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|i| i.label).collect()
}

/// `fields.Char(compute="...", depends=["partner_id.disp"])`: the `depends` kwarg should
/// offer nested-field completion on the last dotted segment, exactly like `related=`.
pub fn test_depends_kwarg_nested_field_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {}", test_file);

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Line `    partner_display_name_dep = fields.Char(compute="_compute_partner_display_name_dep", depends=["partner_id.disp"])`
    // (0-indexed line 79), cursor right after "disp" inside the string.
    let response = CompletionFeature::autocomplete(session, file_symbol, file_info, None, 79, 113);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "display_name"), "Expected display_name to be suggested for the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "create_uid"), "create_uid does not match the 'disp' prefix, got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "id"), "id does not match the 'disp' prefix, got: {:?}", labels);
}

/// The element of a list or a tuple is completed with the cursor at its end, when no type is expected.
/// e.g. `companies = [self.company_id]`, with the cursor after `company_id`
pub fn test_list_element_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let disk_text = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // The line holding the element, and the element, completed with the cursor at its end
    let cases = [
        ("companies = [self.company_id]", "self.company_id"),
        // The first element of a tuple
        ("company_pairs = (self.company_id, self.company_id)", "self.company_id"),
    ];
    for (line_text, element) in cases {
        let (line, text) = disk_text.lines().enumerate().find(|(_, text)| text.contains(line_text)).unwrap();
        let character = (text.find(element).unwrap() + element.len()) as u32;
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, line as u32, character));
        assert!(labels.iter().any(|l| l == "company_id"), "Expected `company_id` after `{}` in `{}`, got: {:?}", element, line_text, labels);
    }
}

/// `default=lambda self: ...` stores a `<lambda>` child on the class, but it is not a member:
/// `self.` must never offer it.
pub fn test_lambda_is_not_a_member(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let disk_text = std::fs::read_to_string(&test_file).expect("Test file does not exist");
    // The line that is exactly `return self`, as other lines of the fixture start with it (`return self.create_uid.log`).
    // Don't match on the line ending: the fixture may be checked out with CRLF (git autocrlf on Windows).
    let return_lines: Vec<usize> = disk_text.lines().enumerate()
        .filter(|(_, line)| line.trim() == "return self")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(return_lines.len(), 1, "Expected a single 'return self' line in {}", test_file);
    let return_line = return_lines[0];

    // Open the buffer with the dot typed after `return self`: the completed attribute name is
    // then empty, so every member of LambdaDefaultModel is offered.
    let text: String = disk_text.split_inclusive('\n').enumerate()
        .map(|(index, line)| if index == return_line { line.replacen("return self", "return self.", 1) } else { line.to_string() })
        .collect();
    let did_open_params = lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&test_file),
            language_id: "python".to_string(),
            version: 1,
            text,
        }
    };
    Odoo::handle_did_open(session, did_open_params);

    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor after the dot of `return self.`
    let column = disk_text.lines().nth(return_line).unwrap().find("return self").unwrap() + "return self.".len();
    let response = CompletionFeature::autocomplete(session, file_symbol, file_info, None, return_line as u32, column as u32);
    let labels = labels(response);
    assert!(labels.iter().any(|l| l == "company_id"), "Expected the model fields to be suggested after 'self.', got: {:?}", labels);
    assert!(!labels.iter().any(|l| l == "<lambda>"), "<lambda> is not a member and must not be suggested, got: {:?}", labels);
}

/// `fields.Integer(compute_sql="...")`: the kwarg offers method completion, but only from 19.1 on
pub fn test_compute_sql_kwarg_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor inside the compute_sql value of `test_int = fields.Integer(..., compute_sql="...")`
    session.sync_odoo.version = OdooVersion::new(19, 1, 0);
    let gated_in = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 8, 80));
    assert!(gated_in.iter().any(|l| l == "_compute_something"), "Expected _compute_something to be suggested for compute_sql, got: {:?}", gated_in);
    session.sync_odoo.version = OdooVersion::new(19, 0, 0);
    let gated_out = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 8, 80));
    assert!(!gated_out.iter().any(|l| l == "_compute_something"), "Expected no method completion for compute_sql before 19.1, got: {:?}", gated_out);
}

/// `group_expand=`, `selection=` and the first positional argument of a Selection offer methods
pub fn test_selection_field_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // Cursor after the 4-char prefix of each string on the `state`, `kind` and `label` field lines
    for (line, column, method) in [(85, 34, "_selection_state"), (85, 67, "_expand_states"), (86, 43, "_selection_state")] {
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, line, column));
        assert!(labels.iter().any(|l| l == method), "Expected {method} to be suggested at {line}:{column}, got: {:?}", labels);
    }
    let label_labels = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 87, 29));
    assert!(!label_labels.iter().any(|l| l == "_selection_state"), "The label of a Char is not a method name, got: {:?}", label_labels);
}

/// The operators offered in a domain leaf depend on the Odoo version.
/// e.g. `not =like` and `not =ilike` from saas-18.2
pub fn test_domain_operator_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let content = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    // In the operator string of `self.search([("company_id", "=", 1)])`
    let before_cursor = r#"self.search([("company_id", ""#;
    let (line, text) = content.lines().enumerate().find(|(_, text)| text.contains(before_cursor)).unwrap();
    let character = (text.find(before_cursor).unwrap() + before_cursor.len()) as u32;

    let mut failures = vec![];
    // (version, whether `not =like` and `not =ilike` are offered)
    for (test_version, offered) in [(OdooVersion::new(18, 1, 0), false), (OdooVersion::new(18, 2, 0), true)] {
        session.sync_odoo.version = test_version;
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, line as u32, character));
        for operator in ["=like", "not =like", "not =ilike"] {
            // `=like` is offered in every version
            let expected = offered || operator == "=like";
            if labels.iter().any(|l| l == operator) != expected {
                failures.push(format!("{operator} on {test_version:?}: expected offered: {expected}, got {labels:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "domain operator completion:\n{}", failures.join("\n"));
}

/// `init_storage="..."` offers method completion, but only from 20.0 on
pub fn test_init_storage_kwarg_method_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    session.sync_odoo.version = OdooVersion::new(20, 0, 0);
    let gated_in = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 86, 76));
    assert!(gated_in.iter().any(|l| l == "_init_column_kind"), "Expected _init_column_kind to be suggested for init_storage, got: {:?}", gated_in);
    session.sync_odoo.version = OdooVersion::new(19, 4, 0);
    let gated_out = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, 86, 76));
    assert!(!gated_out.iter().any(|l| l == "_init_column_kind"), "Expected no method completion for init_storage before 20.0, got: {:?}", gated_out);
}

/// `self.create_uid.log`: `create_uid` is added to every model as a many2one to `res.users`, so its
/// attributes are those of `res.users`.
pub fn test_magic_field_comodel_completion(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("to_complete.py").sanitize();
    let content = std::fs::read_to_string(&test_file).expect("Test file does not exist");

    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };

    let before_cursor = "self.create_uid.log";
    let (line, text) = content.lines().enumerate().find(|(_, text)| text.contains(before_cursor)).unwrap();
    let character = (text.find(before_cursor).unwrap() + before_cursor.len()) as u32;
    let labels = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, line as u32, character));
    assert!(labels.iter().any(|l| l == "login"), "Expected `login` of `res.users` after `{before_cursor}`, got: {:?}", labels);
}

/// `import ｏ|ｓ`: the AST holds the NFKC-normalized name `os`, so the typed prefix must be read up
/// to the cursor in the source, not by slicing the normalized name with source offsets.
pub fn test_import_completion_with_normalized_name(session: &mut SessionInfo) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_semantic_tokens").join("models").join("sem_tokens_non_ascii.py").sanitize();
    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol");
    };
    // Cursor right after `ｏ` (UTF-16 columns) in `import ｏｓ` (0-indexed line 3) and in
    // `from ｏｓ import path` (line 4).
    let mut failures = vec![];
    for (line, character) in [(3, 8), (4, 6)] {
        let labels = labels(CompletionFeature::autocomplete(session, file_symbol, file_info, None, line, character));
        let has_os = labels.iter().any(|l| l == "os");
        let others: Vec<&String> = labels.iter().filter(|l| !l.starts_with('o')).collect();
        if !has_os || !others.is_empty() {
            failures.push(format!("line {line}: `os` suggested: {has_os}, {} names not starting with the typed `o`, e.g. {:?}",
                others.len(), &others[..others.len().min(5)]));
        }
    }
    assert!(failures.is_empty(), "Import completion on an NFKC-normalized name:\n{}", failures.join("\n"));
}
