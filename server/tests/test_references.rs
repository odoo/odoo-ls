use std::path::PathBuf;

use lsp_types::{Location, PartialResultParams, Position, WorkDoneProgressParams};
use odoo_ls_server::{core::file_mgr::FileMgr, threads::SessionInfo, utils::PathSanitizer};

use crate::setup::setup::{create_init_session, setup_server};

mod setup;

#[test]
/// Test various calls to GotoReferences
fn test_references() {
    // Setup server and session
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");

    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    //1. reference of a model
    let mut references = get_references(&mut session, &test_file, Position::new(4, 29));
    //as order is not guaranteed (usage of set), check if the expected reference is somewhere in the result list.
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 3, 0);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 4, 12);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 15, 8);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 32, 8);  // self in self.env[...]
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 32, 17);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 33, 8);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 34, 18);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 42, 16);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 43, 16);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 44, 16);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 45, 16);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 9, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 17, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 18, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 19, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 20, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 22, 34);
    assert_in_result(&mut references, "module_2/models/base_test_models.py", 5, 15);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line + 1, r.range.start.character + 1)).collect::<Vec<String>>().join(", ")
    );

    // reference of an attribute
    let mut references = get_references(&mut session, &test_file, Position::new(9, 8));
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 9, 4);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 39, 18);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line + 1, r.range.start.character + 1)).collect::<Vec<String>>().join(", ")
    );

    //reference of a simple variable, including usages inside newly-handled constructs
    let mut references = get_references(&mut session, &test_file, Position::new(52, 4));
    // usage in models.py import
    assert_in_result(&mut references, "module_1/models/models.py", 1, 30);
    //definition
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 52, 0);
    // usage inside lambda body
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 53, 23);
    // usage inside f-string
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 54, 24);
    // usage inside bool operation
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 55, 13);
    // usage inside compare expression
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 56, 14);
    // usage inside list comprehension element
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 57, 16);
    // usage inside dict comprehension key
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 58, 16);
    // usage inside list literal element
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 59, 12);
    // usage inside tuple literal element
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 60, 13);
    // usage inside set literal element
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 61, 11);
    // usage inside dict literal value
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 62, 15);
    // usage as a call argument
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 63, 17);
    // usage as left-hand side of a binary operation
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 64, 12);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line + 1, r.range.start.character + 1)).collect::<Vec<String>>().join(", ")
    );

    //reference of a function parameter, exercising multiple uses of the same name on a single line
    let mut references = get_references(&mut session, &test_file, Position::new(67, 28));
    // parameter declaration
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 67, 25);
    // three same-line usages inside the `if` test
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 68, 7);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 68, 37);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 68, 59);
    // usage in the return statement
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 70, 11);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line + 1, r.range.start.character + 1)).collect::<Vec<String>>().join(", ")
    );

    //reference of a module name in the manifest
    let test_file = test_addons_path.join("module_1").join("__manifest__.py").sanitize();
    let mut references = get_references(&mut session, &test_file, Position::new(3, 18));
    assert_in_result(&mut references, "module_1/__manifest__.py", 3, 13);
    assert_in_result(&mut references, "module_2/__manifest__.py", 13, 17);
    
    // for r in references.iter() {
    //     error!("Reference found at {}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character);
    // }
}

fn assert_in_result(references: &mut Vec<Location>, end_path: &str, line: u32, character: u32) {
    let before_len = references.len();
    references.retain(|r| !(r.uri.as_str().ends_with(end_path) && r.range.start.line == line && r.range.start.character == character));
    let after_len = references.len();
    assert!(before_len > after_len, "Expected reference not found: {}:{}:{}", end_path, line + 1, character + 1);
    assert!(before_len == after_len + 1, "Duplicated reference found: {}:{}:{}", end_path, line + 1, character + 1);
}

fn get_references(session: &mut SessionInfo, path: &String, position: Position)-> Vec<Location> {
    let test_file_uri = FileMgr::pathname2uri(&path);
    let references_params = lsp_types::ReferenceParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: test_file_uri,
            },
            position,
        },
        context: lsp_types::ReferenceContext {
            include_declaration: true,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let references = odoo_ls_server::core::odoo::Odoo::handle_references(session, references_params);
    assert!(references.is_ok(), "Expected Some result from handle_references, got Err");
    let references = references.unwrap();
    assert!(references.is_some(), "Expected Some result from handle_references, got None for {}:{}-{}", path, position.line, position.character);
    references.unwrap()
}
