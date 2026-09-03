use std::{collections::HashMap, path::{Path, PathBuf}};

use lsp_types::{CompletionContext, CompletionResponse, Diagnostic, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier};
use odoo_ls_server::core::{file_mgr::FileMgr, odoo::{Odoo, SyncOdoo}};
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::{PathSanitizer, ToFilePath};

mod setup;
mod test_utils;

#[derive(Clone, Copy, Hash, Eq, PartialEq)]
enum TestDataFiles {
    XTestModelXml,
    XTestModelCsv,
    XTestModelM2oXml,
    XTestModelM2oCsv,
    PyTestModel,
}

fn get_test_data_file_paths() -> HashMap<TestDataFiles, String> {
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons");
    let mut map = HashMap::new();
    map.insert(TestDataFiles::XTestModelXml, test_addons_path.join("module_xml_models_fields/data/x_test_model.xml").to_string_lossy().to_string());
    map.insert(TestDataFiles::XTestModelCsv, test_addons_path.join("module_xml_models_fields/data/x_test_model.csv").to_string_lossy().to_string());
    map.insert(TestDataFiles::XTestModelM2oXml, test_addons_path.join("module_xml_models_fields/data/x_test_model_m2o.xml").to_string_lossy().to_string());
    map.insert(TestDataFiles::XTestModelM2oCsv, test_addons_path.join("module_xml_models_fields/data/x_test_model_m2o.csv").to_string_lossy().to_string());
    map.insert(TestDataFiles::PyTestModel, test_addons_path.join("module_xml_models_fields/models/py_test_model.py").to_string_lossy().to_string());
    map
}

fn collect_diagnostics(session: &mut SessionInfo, file_paths: &HashMap<TestDataFiles, String>) -> HashMap<TestDataFiles, Vec<Diagnostic>> {
    let paths_to_files = file_paths.iter().map(|(k, v)| (v.as_str(), *k)).collect::<HashMap<_, _>>();
    let mut diagnostics_map = HashMap::new();
    let diags = setup::setup::get_diagnostics_for_paths(session, &file_paths.values().cloned().collect::<Vec<_>>());
    for (path, diags) in diags {
        if let Some(&key) = paths_to_files.get(path.as_str()) {
            diagnostics_map.insert(key, diags);
        }
    }
    diagnostics_map
}

fn completion_labels(response: Option<CompletionResponse>) -> Vec<String> {
    let Some(response) = response else {
        return vec![];
    };
    let items = match response {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };
    items.into_iter().map(|item| item.label).collect()
}

fn simulate_file_change(session: &mut SessionInfo, path: &str, content: &str, version: i32) {
    let params = lsp_types::DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: FileMgr::pathname2uri(path),
            version,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: content.to_string(),
        }],
    };
    Odoo::handle_did_change(session, params);
}

fn py_test_source(model_expr: &str, domain_expr: &str) -> String {
    format!(
        "from odoo import api, fields, models, _, Command\n\nclass DemoModel(models.Model): # OLS03303\n    _name = 'x_test_model' # OLS03020\n\n    def method(self):\n        self.env[\"{}\"].search([(\"{}\",\"=\",self.name)])\n",
        model_expr, domain_expr
    )
}

#[test]
fn test_xml_fields() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_paths = get_test_data_file_paths();

    let diagnostics = collect_diagnostics(&mut session, &file_paths);

    // No diagnostics on any of the files
    assert!(diagnostics[&TestDataFiles::XTestModelXml].is_empty(), "Expected no diagnostics for x_test_model.xml, but found: {:?}", diagnostics[&TestDataFiles::XTestModelXml]);
    assert!(diagnostics[&TestDataFiles::XTestModelCsv].is_empty(), "Expected no diagnostics for x_test_model.csv, but found: {:?}", diagnostics[&TestDataFiles::XTestModelCsv]);
    assert!(diagnostics[&TestDataFiles::XTestModelM2oXml].is_empty(), "Expected no diagnostics for x_test_model_m2o.xml, but found: {:?}", diagnostics[&TestDataFiles::XTestModelM2oXml]);
    assert!(diagnostics[&TestDataFiles::XTestModelM2oCsv].is_empty(), "Expected no diagnostics for x_test_model_m2o.csv, but found: {:?}", diagnostics[&TestDataFiles::XTestModelM2oCsv]);

    // Look for OLS03303 diagnostic on py_test_model.py
    let doc_diags = setup::setup::get_diagnostics_test_comments(&mut session, &file_paths[&TestDataFiles::PyTestModel]);
    test_utils::verify_diagnostics_against_doc(&diagnostics[&TestDataFiles::PyTestModel], &doc_diags);
}

#[test]
fn test_xml_fields_def_hover_completion() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_paths = get_test_data_file_paths();

    let x_test_model_xml_path = Path::new(&file_paths[&TestDataFiles::XTestModelXml]);
    let x_test_model_xml = x_test_model_xml_path.sanitize_cow();
    let x_test_model_csv_path = Path::new(&file_paths[&TestDataFiles::XTestModelCsv]);
    let x_test_model_csv = x_test_model_csv_path.sanitize_cow();
    let x_test_model_m2o_xml_path = Path::new(&file_paths[&TestDataFiles::XTestModelM2oXml]);
    let x_test_model_m2o_xml = x_test_model_m2o_xml_path.sanitize_cow();
    let x_test_model_m2o_csv_path = Path::new(&file_paths[&TestDataFiles::XTestModelM2oCsv]);
    let x_test_model_m2o_csv = x_test_model_m2o_csv_path.sanitize_cow();
    let py_file_path = Path::new(&file_paths[&TestDataFiles::PyTestModel]);
    let py_file_str = py_file_path.sanitize_cow();

    // ---------- CSV definition checks ----------
    let x_test_model_csv_info = session.file_mgr().get_file_info(&x_test_model_csv).unwrap();
    let Some(x_test_model_csv_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        x_test_model_csv_path,
    ) else {
        panic!("Failed to get symbol for {}", x_test_model_csv);
    };

    // Header x_name should go to field definition in x_test_model.xml
    let x_name_defs = test_utils::get_definition_locs(
        &mut session,
        x_test_model_csv_symbol,
        x_test_model_csv_info,
        0,
        5,
    );
    assert!(
        x_name_defs
            .iter()
            .any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == x_test_model_xml),
        "Expected x_name definition from x_test_model.csv to point to {}, got: {:?}",
        x_test_model_xml,
        x_name_defs
            .iter()
            .map(|loc| loc.target_uri.to_file_path().unwrap().sanitize())
            .collect::<Vec<_>>()
    );

    // CSV xml_id should resolve to itself (same-file reference)
    let test_record_defs = test_utils::get_definition_locs(
        &mut session,
        x_test_model_csv_symbol,
        x_test_model_csv_info,
        1,
        8,
    );
    assert_eq!(test_record_defs.len(), 1, "Expected exactly one definition for test_xml_test_record in CSV");
    assert_eq!(
        test_record_defs[0].target_uri.to_file_path().unwrap().sanitize(),
        x_test_model_csv,
        "Expected test_xml_test_record to resolve in same CSV file"
    );
    assert_eq!(test_record_defs[0].target_range.start.line, 1);
    assert_eq!(test_record_defs[0].target_range.start.character, 0);
    assert_eq!(test_record_defs[0].target_range.end.character, 20);

    let x_test_model_m2o_csv_info = session.file_mgr().get_file_info(&x_test_model_m2o_csv).unwrap();
    let Some(x_test_model_m2o_csv_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        x_test_model_m2o_csv_path,
    ) else {
        panic!("Failed to get symbol for {}", x_test_model_m2o_csv);
    };

    // Header x_name should go to field definition in x_test_model_m2o.xml
    let x_name_m2o_defs = test_utils::get_definition_locs(
        &mut session,
        x_test_model_m2o_csv_symbol,
        x_test_model_m2o_csv_info,
        0,
        5,
    );
    assert!(
        x_name_m2o_defs
            .iter()
            .any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == x_test_model_m2o_xml),
        "Expected x_name definition from x_test_model_m2o.csv to point to {}, got: {:?}",
        x_test_model_m2o_xml,
        x_name_m2o_defs
            .iter()
            .map(|loc| loc.target_uri.to_file_path().unwrap().sanitize())
            .collect::<Vec<_>>()
    );

    // CSV xml_id should resolve to itself (same-file reference)
    let test_m2o_record_defs = test_utils::get_definition_locs(
        &mut session,
        x_test_model_m2o_csv_symbol,
        x_test_model_m2o_csv_info,
        1,
        10,
    );
    assert_eq!(
        test_m2o_record_defs.len(),
        1,
        "Expected exactly one definition for test_xml_test_m2o_record in CSV"
    );
    assert_eq!(
        test_m2o_record_defs[0].target_uri.to_file_path().unwrap().sanitize(),
        x_test_model_m2o_csv,
        "Expected test_xml_test_m2o_record to resolve in same CSV file"
    );
    assert_eq!(test_m2o_record_defs[0].target_range.start.line, 1);
    assert_eq!(test_m2o_record_defs[0].target_range.start.character, 0);
    assert_eq!(test_m2o_record_defs[0].target_range.end.character, 24);

    // ---------- Python hover + definition checks ----------
    let py_file_info = session.file_mgr().get_file_info(&py_file_str).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        py_file_path,
    ) else {
        panic!("Failed to get symbol for {}", py_file_str);
    };

    // Hover model name in self.env["x_test_model_m2o"]
    let hover_model = test_utils::get_hover_markdown(&mut session, py_file_symbol, py_file_info, 6, 24)
        .unwrap_or_else(|| panic!("Expected hover content on model name"));
    assert!(hover_model.contains("x_test_model_m2o"), "Model hover should include x_test_model_m2o; got:\n{}", hover_model);

    // Hover on x_other_model and x_name in domain string
    let hover_x_other_model = test_utils::get_hover_markdown(&mut session, py_file_symbol, py_file_info, 6, 52)
        .unwrap_or_else(|| panic!("Expected hover content on x_other_model"));
    assert!(hover_x_other_model.contains("XML record"), "Hover on x_other_model should return an XML-backed record description; got:\n{}", hover_x_other_model);
    assert!(hover_x_other_model.contains("m2o_field"), "Hover on x_other_model should reference m2o_field; got:\n{}", hover_x_other_model);

    let hover_x_name = test_utils::get_hover_markdown(&mut session, py_file_symbol, py_file_info, 6, 63)
        .unwrap_or_else(|| panic!("Expected hover content on x_name"));
    assert!(hover_x_name.contains("XML record"), "Hover on x_name should return an XML-backed record description; got:\n{}", hover_x_name);

    // Hover on search() call itself should return something useful
    let hover_search = test_utils::get_hover_markdown(&mut session, py_file_symbol, py_file_info, 6, 39)
        .unwrap_or_else(|| panic!("Expected hover content on search"));
    assert!(hover_search.contains("search"), "Hover on search should mention search; got:\n{}", hover_search);

    // Definition from Python model and fields should resolve to XML model/field records
    let py_model_defs = test_utils::get_definition_locs(&mut session, py_file_symbol, py_file_info, 6, 24);
    assert!(
        py_model_defs
            .iter()
            .any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == x_test_model_m2o_xml),
        "Expected model definition from Python to point to {}; got: {:?}",
        x_test_model_m2o_xml,
        py_model_defs
            .iter()
            .map(|loc| loc.target_uri.to_file_path().unwrap().sanitize())
            .collect::<Vec<_>>()
    );

    let py_x_other_model_defs = test_utils::get_definition_locs(&mut session, py_file_symbol, py_file_info, 6, 52);
    assert!(
        py_x_other_model_defs
            .iter()
            .any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == x_test_model_m2o_xml),
        "Expected x_other_model definition from Python to point to {}; got: {:?}",
        x_test_model_m2o_xml,
        py_x_other_model_defs
            .iter()
            .map(|loc| loc.target_uri.to_file_path().unwrap().sanitize())
            .collect::<Vec<_>>()
    );

    let py_x_name_defs = test_utils::get_definition_locs(&mut session, py_file_symbol, py_file_info, 6, 63);
    assert!(
        py_x_name_defs
            .iter()
            .any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == x_test_model_xml),
        "Expected x_name definition from Python to point to {}; got: {:?}",
        x_test_model_xml,
        py_x_name_defs
            .iter()
            .map(|loc| loc.target_uri.to_file_path().unwrap().sanitize())
            .collect::<Vec<_>>()
    );

    // ---------- Completion checks ----------
    // 1) Completion for model names when prefix is x_
    let py_source_model_prefix = py_test_source("x_", "x_other_model.x_name");
    simulate_file_change(&mut session, &py_file_str, &py_source_model_prefix, 2);

    let py_file_info = session.file_mgr().get_file_info(&py_file_str).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        py_file_path,
    ) else {
        panic!("Failed to get symbol for {} after didChange", py_file_str);
    };
    let model_completion_labels = completion_labels(CompletionFeature::autocomplete(
        &mut session,
        py_file_symbol,
        py_file_info,
        Some(CompletionContext {
            trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(".".to_string()),
        }),
        6,
        19,
    ));
    assert!(
        model_completion_labels.iter().any(|label| label == "x_test_model_m2o"),
        "Model completion at x_ should include x_test_model_m2o; got: {:?}",
        model_completion_labels
    );

    // 2) Completion for domain field prefix at search([("x_
    let py_source_domain_prefix = py_test_source("x_test_model_m2o", "x_");
    simulate_file_change(&mut session, &py_file_str, &py_source_domain_prefix, 3);

    let py_file_info = session.file_mgr().get_file_info(&py_file_str).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        py_file_path,
    ) else {
        panic!("Failed to get symbol for {} after didChange", py_file_str);
    };
    let domain_field_labels = completion_labels(CompletionFeature::autocomplete(
        &mut session,
        py_file_symbol,
        py_file_info,
        Some(CompletionContext {
            trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(".".to_string()),
        }),
        6,
        48,
    ));
    assert!(
        domain_field_labels.iter().any(|label| label == "x_other_model"),
        "Domain completion at x_ should include x_other_model; got: {:?}",
        domain_field_labels
    );

    // 3) Completion for nested field prefix at search([("x_other_model.x
    let py_source_nested_prefix = py_test_source("x_test_model_m2o", "x_other_model.x");
    simulate_file_change(&mut session, &py_file_str, &py_source_nested_prefix, 4);

    let py_file_info = session.file_mgr().get_file_info(&py_file_str).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        py_file_path,
    ) else {
        panic!("Failed to get symbol for {} after didChange", py_file_str);
    };
    let nested_field_labels = completion_labels(CompletionFeature::autocomplete(
        &mut session,
        py_file_symbol,
        py_file_info,
        Some(CompletionContext {
            trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(".".to_string()),
        }),
        6,
        61,
    ));
    assert!(
        nested_field_labels.iter().any(|label| label == "x_name"),
        "Nested field completion at x_other_model.x should include x_name; got: {:?}",
        nested_field_labels
    );

    // 4) Completion for delegated fields through `_inherits` delegation inheritance.
    //    `x_delegating_model` delegates to `x_parent_model` (see delegation_model.py),
    //    so a search domain on the delegating model must surface the delegated field
    //    `parent_only_field` next to its own fields (`parent_id`, `own_field`).
    let py_source_inherits = py_test_source("x_delegating_model", "parent_");
    simulate_file_change(&mut session, &py_file_str, &py_source_inherits, 5);

    let py_file_info = session.file_mgr().get_file_info(&py_file_str).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        py_file_path,
    ) else {
        panic!("Failed to get symbol for {} after didChange", py_file_str);
    };
    let inherits_field_labels = completion_labels(CompletionFeature::autocomplete(
        &mut session,
        py_file_symbol,
        py_file_info,
        Some(CompletionContext {
            trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(".".to_string()),
        }),
        6,
        53,
    ));
    assert!(
        inherits_field_labels.iter().any(|label| label == "parent_only_field"),
        "Domain completion on a model with `_inherits` should include the delegated field parent_only_field; got: {:?}",
        inherits_field_labels
    );
    assert!(
        inherits_field_labels.iter().any(|label| label == "parent_id"),
        "Domain completion should still include the delegation field parent_id; got: {:?}",
        inherits_field_labels
    );
}
