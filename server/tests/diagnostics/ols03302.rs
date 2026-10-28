use crate::setup::shared::with_shared_session;
use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{verify_diagnostics_against_doc}};

#[test]
fn test_ols03302() {
    // Setup server and session with test addons
    with_shared_session(true, |session| {
        let path = env::current_dir().unwrap().join("tests/data/addons/module_2/__manifest__.py").sanitize();
        let diagnostics = get_diagnostics_for_path(session, &path);
        let doc_diags = get_diagnostics_test_comments(session, &path);
        verify_diagnostics_against_doc(&diagnostics, &doc_diags);
    });
}
