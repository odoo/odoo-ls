use crate::setup::shared::with_shared_session;
use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{diag_on_line, verify_diagnostics_against_doc}};

#[test]
fn test_ols03001_23() {
    // Setup server and session with test addons
    with_shared_session(true, |session| {
        let path = env::current_dir().unwrap().join("tests/data/addons/module_1/models/diagnostics.py").sanitize();
        let diagnostics = get_diagnostics_for_path(session, &path);
        let doc_diags = get_diagnostics_test_comments(session, &path);

        // Verify that there are no OLS03022 diagnostics on line 130 (index 129) for a O2M field with inverse name to a Many2oneReference field
        let line_diagnostics = diag_on_line(&diagnostics, 129);
        assert_eq!(line_diagnostics.len(), 0, "Expected no diagnostics on line 130, but found some: {:?}", line_diagnostics);
        // Verify all diagnostics against those specified in the document
        verify_diagnostics_against_doc(&diagnostics, &doc_diags);
    });
}
