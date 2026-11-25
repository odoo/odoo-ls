use std::env;

use odoo_ls_server::{threads::SessionInfo, utils::PathSanitizer};

use crate::common::*;

pub fn test_ols03301(mut session: SessionInfo) {
    let path = env::current_dir().unwrap().join("tests/data/addons/module_2/models/base_test_models.py").sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let doc_diags = get_diagnostics_test_comments(&mut session, &path);
    verify_diagnostics_against_doc(&diagnostics, &doc_diags);
}
