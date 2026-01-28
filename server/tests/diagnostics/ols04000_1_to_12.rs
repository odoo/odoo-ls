use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{verify_diagnostics_against_doc}};


#[test]
fn test_ols04001_to_12() {
    // Setup server and session with test addons
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let paths = vec![
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04001/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04002/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04003_4/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04005/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04006/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04007/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04008/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04009/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04010/__manifest__.py").sanitize(),
        env::current_dir().unwrap().join("tests/data/addons/manifest_module_04012/__manifest__.py").sanitize(),
    ];
    let diagnostics_map = get_diagnostics_for_paths(&mut session, &paths);
    for path in paths.iter() {
        let doc_diags = get_diagnostics_test_comments(&mut session, &path);
        let diagnostics = diagnostics_map.get(path).cloned().unwrap_or_default();
        verify_diagnostics_against_doc(diagnostics, doc_diags);
    }
}