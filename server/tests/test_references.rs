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
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 4, 12);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 15, 8);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 32, 17);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 33, 8);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 34, 18);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 40, 16);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 41, 16);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 42, 16);
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 43, 16);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 9, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 17, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 18, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 19, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 20, 34);
    assert_in_result(&mut references, "module_1/models/diagnostics.py", 22, 34);
    assert_in_result(&mut references, "module_2/models/base_test_models.py", 5, 15);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character)).collect::<Vec<String>>().join(", ")
    );

    // reference of an attribute
    let mut references = get_references(&mut session, &test_file, Position::new(9, 8));
    assert_in_result(&mut references, "module_1/models/base_test_models.py", 37, 18);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character)).collect::<Vec<String>>().join(", ")
    );

    //reference of a simple variable
    let mut references = get_references(&mut session, &test_file, Position::new(50, 4));
    assert_in_result(&mut references, "module_1/models/models.py", 1, 30);
    assert!(references.len() == 0, "Some references were not expected: {}",
        references.iter().map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character)).collect::<Vec<String>>().join(", ")
    );
    
    // for r in references.iter() {
    //     error!("Reference found at {}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character);
    // }
}

fn assert_in_result(references: &mut Vec<Location>, end_path: &str, line: u32, character: u32) {
    let mut index = None;
    for (i, r) in references.iter().enumerate() {
        if r.uri.as_str().ends_with(end_path) && r.range.start.line == line && r.range.start.character == character {
            index = Some(i);
        }
    }
    if let Some(i) = index {
        references.remove(i);
    } else {
        assert!(false, "Expected reference not found: {}:{}:{}", end_path, line, character);
    }
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
