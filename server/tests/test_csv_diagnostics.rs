// Test CSV diagnostics: field count mismatches, parsing errors, and xml_id format validation

use lsp_types::Diagnostic;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use odoo_ls_server::utils::HashMap;
use std::env;
use std::path::Path;

mod setup;
mod test_utils;

fn csv_test_paths() -> (String, String, String) {
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons");

    let field_mismatch = test_addons_path
        .join("module_csv").join("data").join("csv_field_mismatch").join("res.country.state.csv")
        .sanitize();
    let invalid_xml_id = test_addons_path
        .join("module_csv").join("data").join("csv_invalid_xml_id").join("res.country.state.csv")
        .sanitize();
    let valid_csv = test_addons_path
        .join("module_for_diagnostics").join("data").join("bike_parts.wheel.csv")
        .sanitize();

    for path in [&field_mismatch, &invalid_xml_id, &valid_csv] {
        assert!(Path::new(path).exists(), "Test file does not exist: {}", path);
    }

    (field_mismatch, invalid_xml_id, valid_csv)
}

fn collect_all_csv_diagnostics(session: &mut SessionInfo, paths: &[&str]) -> HashMap<String, Vec<Diagnostic>> {
    let paths_vec: Vec<_> = paths.iter().map(|s| s.to_string()).collect();
    setup::setup::get_diagnostics_for_paths(session, &paths_vec)
}

fn get_diags_for<'a>(all: &'a HashMap<String, Vec<Diagnostic>>, path: &str) -> &'a [Diagnostic] {
    all.get(path).map(|v| v.as_slice()).unwrap_or(&[])
}

fn has_code(diag: &Diagnostic, code: &str) -> bool {
    diag.code.as_ref().map(|c| match c {
        lsp_types::NumberOrString::String(s) => s.contains(code),
        lsp_types::NumberOrString::Number(_) => false,
    }).unwrap_or(false)
}

#[test]
fn test_csv_diagnostics() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let (field_mismatch, invalid_xml_id, valid_csv) = csv_test_paths();

    // Collect all diagnostics in one pass (consuming the message queue once)
    let all_diags = collect_all_csv_diagnostics(
        &mut session,
        &[&field_mismatch, &invalid_xml_id, &valid_csv],
    );

    test_field_count_mismatch(get_diags_for(&all_diags, &field_mismatch));
    test_xml_id_format(get_diags_for(&all_diags, &invalid_xml_id));
    test_valid_file_no_errors(get_diags_for(&all_diags, &valid_csv));
}

/// OLS05069: header has 4 fields, data row has 3
/// File: csv_field_mismatch/res.country.state.csv
///   Line 0: id,country_id:id,name,code
///   Line 1: state_mismatch_1,base.au,Test State
fn test_field_count_mismatch(diagnostics: &[Diagnostic]) {
    // Must have at least one OLS05069
    let mismatch_diags: Vec<_> = diagnostics.iter().filter(|d| has_code(d, "OLS05069")).collect();
    assert!(
        !mismatch_diags.is_empty(),
        "Expected OLS05069 field mismatch diagnostic, got: {:?}",
        diagnostics
    );

    for diag in &mismatch_diags {
        // Message should mention field counts
        assert!(
            diag.message.contains("field"),
            "OLS05069 message should mention 'field', got: {}",
            diag.message
        );

        // Severity should be Warning (per diagnostic_codes_list.rs)
        assert_eq!(
            diag.severity,
            Some(lsp_types::DiagnosticSeverity::WARNING),
            "OLS05069 should be a warning"
        );

        // The mismatch is on the data row (line 1, 0-indexed)
        assert!(
            diag.range.start.line >= 1,
            "Field mismatch diagnostic should be on the data row (line >= 1), got line {}",
            diag.range.start.line
        );
    }

    // Verify all diagnostics have well-formed ranges
    for diag in diagnostics {
        assert!(diag.code.is_some(), "Diagnostic should have a code");
        assert!(!diag.message.is_empty(), "Diagnostic should have a message");
        assert!(
            diag.range.start <= diag.range.end,
            "Range start should be <= end: {:?}",
            diag.range
        );
    }
}

/// OLS05051: xml_id has more than one dot → "module.sub.bad_xml_id"
/// File: csv_invalid_xml_id/res.country.state.csv
///   Line 0: id,country_id:id,name,code
///   Line 1: module.sub.bad_xml_id,base.au,Test State,TS1
fn test_xml_id_format(diagnostics: &[Diagnostic]) {
    let xml_id_diags: Vec<_> = diagnostics.iter().filter(|d| has_code(d, "OLS05051")).collect();
    assert!(
        !xml_id_diags.is_empty(),
        "Expected OLS05051 xml_id format diagnostic, got: {:?}",
        diagnostics
    );

    for diag in &xml_id_diags {
        // The bad xml_id record is on line 1
        assert!(
            diag.range.start.line >= 1,
            "OLS05051 diagnostic should be on the data row (line >= 1), got line {}",
            diag.range.start.line
        );
    }
}

/// Well-formed CSV: no error-level diagnostics expected
/// File: bike_parts.wheel.csv
///   Line 0: id,name,price
///   Line 1: bike_wheel_6,Road Bike Wheel2,200.0
fn test_valid_file_no_errors(diagnostics: &[Diagnostic]) {
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "Well-formed CSV should not have error diagnostics, got: {}",
        errors
            .iter()
            .map(|d| format!(
                "{}: {}",
                d.code.as_ref().map(|c| format!("{:?}", c)).unwrap_or_default(),
                d.message
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );
}

/// A relational `/id` column lists its ids comma separated, each one validated on its own.
#[test]
fn test_csv_relational_id_column_is_a_list() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("data").join("addons")
        .join("module_for_diagnostics").join("data").join("bikes.bike.csv")
        .sanitize();

    let diagnostics = setup::setup::get_diagnostics_for_path(&mut session, &path);
    let unknown = diagnostics.iter().filter(|diag| has_code(diag, "OLS05001")).collect::<Vec<_>>();
    assert_eq!(unknown.len(), 1, "only the one bogus id is unknown, got: {diagnostics:?}");

    // The range spans that id alone, not the whole cell, so the ids beside it stay untouched.
    let content = std::fs::read_to_string(&path).unwrap();
    let range = &unknown[0].range;
    let line = content.lines().nth(range.start.line as usize).expect("diagnostic line");
    assert_eq!(&line[range.start.character as usize..range.end.character as usize], "bike_wheel_DOES_NOT_EXIST");
}
