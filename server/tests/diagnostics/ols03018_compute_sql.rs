use std::fs;
use std::path::{Path, PathBuf};

use lsp_types::{NumberOrString, TextDocumentContentChangeEvent};
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::diag_on_line};

const VALID_COMPUTE_SQL_LINE: u32 = 7;
const MISSING_COMPUTE_SQL_LINE: u32 = 8;
const MISSING_INIT_STORAGE_LINE: u32 = 9;

/// Replays didChange, the only path that unloads the symbols and re-runs the field init hook
fn reload(session: &mut SessionInfo, path: &str, text: String, version: i32) {
    let event = [TextDocumentContentChangeEvent { range: None, range_length: None, text }];
    session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path, Some(event.as_slice()), Some(version), false);
    Odoo::update_file_index(session, Path::new(path), "py", false, false);
}

/// OLS03018: compute_sql and init_storage name a missing method, only checked from 19.1 and 20.0 on
#[test]
fn test_ols03018_compute_sql_method() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/addons/module_for_diagnostics/models/compute_sql.py").sanitize();
    let source = fs::read_to_string(&path).expect("unable to read compute_sql.py");
    let _ = get_diagnostics_for_path(&mut session, &path);

    // the kwarg is not a field argument before 19.1, so nothing it names may be checked
    session.sync_odoo.version = OdooVersion::new(19, 0, 0);
    reload(&mut session, &path, format!("{source}\n"), 2);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let on_missing = diag_on_line(&diagnostics, MISSING_COMPUTE_SQL_LINE);
    assert!(on_missing.is_empty(), "expected no diagnostic on line {} for version 19.0, got: {:?}", MISSING_COMPUTE_SQL_LINE + 1, on_missing);

    session.sync_odoo.version = OdooVersion::new(19, 1, 0);
    reload(&mut session, &path, source.clone(), 3);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let on_missing = diag_on_line(&diagnostics, MISSING_COMPUTE_SQL_LINE);
    assert_eq!(on_missing.len(), 1, "expected exactly one diagnostic on line {} for version 19.1, got: {:?}", MISSING_COMPUTE_SQL_LINE + 1, on_missing);
    assert!(matches!(&on_missing[0].code, Some(NumberOrString::String(c)) if c == "OLS03018"), "expected OLS03018 on line {}, got: {:?}", MISSING_COMPUTE_SQL_LINE + 1, on_missing[0].code);
    let on_missing_init = diag_on_line(&diagnostics, MISSING_INIT_STORAGE_LINE);
    assert!(on_missing_init.is_empty(), "expected no diagnostic on line {} for version 19.1, got: {:?}", MISSING_INIT_STORAGE_LINE + 1, on_missing_init);

    // a compute_sql naming an existing method stays clean on the version that checks it
    let on_valid = diag_on_line(&diagnostics, VALID_COMPUTE_SQL_LINE);
    assert!(on_valid.is_empty(), "expected no diagnostic on line {} for an existing compute_sql method, got: {:?}", VALID_COMPUTE_SQL_LINE + 1, on_valid);

    session.sync_odoo.version = OdooVersion::new(20, 0, 0);
    reload(&mut session, &path, format!("{source}\n\n"), 4);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let on_missing_init = diag_on_line(&diagnostics, MISSING_INIT_STORAGE_LINE);
    assert_eq!(on_missing_init.len(), 1, "expected exactly one diagnostic on line {} for version 20.0, got: {:?}", MISSING_INIT_STORAGE_LINE + 1, on_missing_init);
    assert!(matches!(&on_missing_init[0].code, Some(NumberOrString::String(c)) if c == "OLS03018"), "expected OLS03018 on line {}, got: {:?}", MISSING_INIT_STORAGE_LINE + 1, on_missing_init[0].code);
}
