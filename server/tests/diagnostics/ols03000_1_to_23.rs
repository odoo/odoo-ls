use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{verify_diagnostics_against_doc}};

#[test]
fn test_ols03001() {
    // Setup server and session with test addons
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/addons/module_1/models/diagnostics.py").sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let doc_diags = get_diagnostics_test_comments(&mut session, &path);
    verify_diagnostics_against_doc(diagnostics, doc_diags);
}