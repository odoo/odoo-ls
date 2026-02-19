use std::path::PathBuf;

use lsp_types::{Location, PartialResultParams, Position, TextDocumentContentChangeEvent, WorkDoneProgressParams};
use odoo_ls_server::{core::file_mgr::FileMgr, features::references, threads::SessionInfo, utils::PathSanitizer};
use tracing::error;

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
    let references = get_references(&mut session, &test_file, Position::new(4, 29));
    for r in references.iter() {
        error!("Reference found at {}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character);
    }
    assert!(references.len() >= 10, "Expected at least 10 references, got {}", references.len());
    for i in 0..9 {
        assert!(references[i].uri.as_str().ends_with("base_test_models.py"), "Expected reference in base_test_models.py, got {}", references[i].uri.as_str());
    }
    assert!(references[0].range.start.line == 4 && references[0].range.start.character == 12, "Expected 1th reference at line 4, character 12, got line {}, character {}", references[0].range.start.line, references[0].range.start.character);
    assert!(references[1].range.start.line == 15 && references[1].range.start.character == 8, "Expected 2nd reference at line 15, character 8, got line {}, character {}", references[1].range.start.line, references[1].range.start.character);
    assert!(references[2].range.start.line == 32 && references[2].range.start.character == 17, "Expected 3th reference at line 32, character 17, got line {}, character {}", references[2].range.start.line, references[2].range.start.character);
    assert!(references[3].range.start.line == 33 && references[3].range.start.character == 8, "Expected 4th reference at line 33, character 8, got line {}, character {}", references[3].range.start.line, references[3].range.start.character);
    assert!(references[4].range.start.line == 34 && references[4].range.start.character == 18, "Expected 5th reference at line 34, character 18, got line {}, character {}", references[4].range.start.line, references[4].range.start.character);
    assert!(references[5].range.start.line == 40 && references[5].range.start.character == 16, "Expected 6th reference at line 40, character 16, got line {}, character {}", references[5].range.start.line, references[5].range.start.character);
    assert!(references[6].range.start.line == 41 && references[6].range.start.character == 16, "Expected 7th reference at line 41, character 16, got line {}, character {}", references[6].range.start.line, references[6].range.start.character);
    assert!(references[7].range.start.line == 42 && references[7].range.start.character == 16, "Expected 8th reference at line 42, character 16, got line {}, character {}", references[7].range.start.line, references[7].range.start.character);
    assert!(references[8].range.start.line == 43 && references[8].range.start.character == 16, "Expected 9th reference at line 43, character 16, got line {}, character {}", references[8].range.start.line, references[8].range.start.character);
    assert!(references[9].uri.as_str().ends_with("module_2/models/base_test_models.py"), "Expected reference 10 to be in module2/base_test_models.py, got {}", references[9].uri.as_str());
    assert!(references[10].range.start.line == 44 && references[10].range.start.character == 16, "Expected ninth reference at line 44, character 16, got line {}, character {}", references[10].range.start.line, references[10].range.start.character);
    assert!(references.len() == 11, "Expected only 10 references, got unexpected additional references: {}",
        references.iter().skip(11).map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character)).collect::<Vec<String>>().join(", ")
    );

    // reference of an attribute
    let references = get_references(&mut session, &test_file, Position::new(9, 8));
    for r in references.iter() {
        error!("Reference found at {}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character);
    }
    assert!(references.len() >= 8, "Expected at least 8 references, got {}", references.len());
    for i in 0..8 {
        assert!(references[i].uri.as_str().ends_with("base_test_models.py"), "Expected reference in base_test_models.py, got {}", references[i].uri.as_str());
    }
    assert!(references[0].range.start.line == 15 && references[0].range.start.character == 8, "Expected first reference at line 15, character 8, got line {}, character {}", references[0].range.start.line, references[0].range.start.character);
    assert!(references[1].range.start.line == 32 && references[1].range.start.character == 8, "Expected second reference at line 32, character 8, got line {}, character {}", references[1].range.start.line, references[1].range.start.character);
    assert!(references[2].range.start.line == 33 && references[2].range.start.character == 8, "Expected third reference at line 33, character 8, got line {}, character {}", references[2].range.start.line, references[2].range.start.character);
    assert!(references[3].range.start.line == 34 && references[3].range.start.character == 18, "Expected fourth reference at line 34, character 18, got line {}, character {}", references[3].range.start.line, references[3].range.start.character);
    assert!(references[4].range.start.line == 40 && references[4].range.start.character == 16, "Expected fifth reference at line 40, character 16, got line {}, character {}", references[4].range.start.line, references[4].range.start.character);
    assert!(references[5].range.start.line == 41 && references[5].range.start.character == 16, "Expected sixth reference at line 41, character 16, got line {}, character {}", references[5].range.start.line, references[5].range.start.character);
    assert!(references[6].range.start.line == 42 && references[6].range.start.character == 16, "Expected seventh reference at line 42, character 16, got line {}, character {}", references[6].range.start.line, references[6].range.start.character);
    assert!(references[7].range.start.line == 43 && references[7].range.start.character == 16, "Expected eighth reference at line 43, character 16, got line {}, character {}", references[7].range.start.line, references[7].range.start.character);
    assert!(references.len() == 8, "Expected only 8 references, got unexpected additional references: {}",
        references.iter().skip(8).map(|r| format!("{}:{}:{}", r.uri.as_str(), r.range.start.line, r.range.start.character)).collect::<Vec<String>>().join(", ")
    );

    //reference of a simple variable
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
