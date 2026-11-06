use crate::setup::shared::with_shared_session;
use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{verify_diagnostics_against_doc}};

#[test]
fn test_ols01008() {
    with_shared_session(false, |session| {
        let path = env::current_dir().unwrap().join("tests/data/python/diagnostics/ols01008.py").sanitize();
        prepare_custom_entry_point(session, &path);
        let diagnostics = get_diagnostics_for_path(session, &path);
        let doc_diags = get_diagnostics_test_comments(session, &path);
        verify_diagnostics_against_doc(&diagnostics, &doc_diags);
    });
}
