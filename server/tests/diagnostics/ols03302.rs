use std::env;

use odoo_ls_server::{threads::SessionInfo, utils::PathSanitizer};

use crate::common::*;

pub fn test_ols03302(mut session: SessionInfo) {
    let path = env::current_dir().unwrap().join("tests/data/addons/module_2/__manifest__.py").sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let doc_diags = get_diagnostics_test_comments(&mut session, &path);
    verify_diagnostics_against_doc(&diagnostics, &doc_diags);
}
