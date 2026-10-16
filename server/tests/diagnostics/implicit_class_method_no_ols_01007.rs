use crate::setup::shared::with_shared_session;
use odoo_ls_server::utils::PathSanitizer;
use std::env;

use crate::{setup::setup::*, test_utils::diag_on_line};

#[test]
fn implicit_class_method_no_ols_01007() {
    // Setup server and session with test addons
    with_shared_session(false, |session| {
        let path = env::current_dir()
            .unwrap()
            .join("tests/data/python/lack_of_diagnostics/implicit_class_method_no_ols_01007.py")
            .sanitize();
        prepare_custom_entry_point(session, &path);
        let diagnostics = get_diagnostics_for_path(session, &path);
        // Verify that there are no OLS01007 on line 4 due to implicit classmethod of `__init_subclass__`
        let line_diagnostics = diag_on_line(&diagnostics, 3);
        assert_eq!(
            line_diagnostics.len(),
            0,
            "Expected no diagnostics on line 4, but found some: {:?}",
            line_diagnostics
        );
    });
}
