use odoo_ls_server::{threads::SessionInfo, utils::PathSanitizer};
use std::env;

use crate::common::*;

pub fn implicit_class_method_no_ols_01007(mut session: SessionInfo) {
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/lack_of_diagnostics/implicit_class_method_no_ols_01007.py")
        .sanitize();
    prepare_custom_entry_point(&mut session, &path);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    // Verify that there are no OLS01007 on line 4 due to implicit classmethod of `__init_subclass__`
    let line_diagnostics = diag_on_line(&diagnostics, 3);
    assert_eq!(
        line_diagnostics.len(),
        0,
        "Expected no diagnostics on line 4, but found some: {:?}",
        line_diagnostics
    );
}
