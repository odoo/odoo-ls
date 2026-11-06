use crate::setup::shared::with_shared_session;
use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::{verify_diagnostics_against_doc}};


#[test]
fn test_ols04001_to_20() {
    // Setup server and session with test addons
    with_shared_session(true, |session| {
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
            //env::current_dir().unwrap().join("tests/data/addons/manifest_module_04011/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04012/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04013/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04014/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04015/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04016/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04017/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04018/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04019/__manifest__.py").sanitize(),
            env::current_dir().unwrap().join("tests/data/addons/manifest_module_04020/__manifest__.py").sanitize(),
        ];
        let diagnostics_map = get_diagnostics_for_paths(session, &paths);
        for path in paths.iter() {
            let doc_diags = get_diagnostics_test_comments(session, path);
            let diagnostics = diagnostics_map.get(path).cloned().unwrap_or_default();
            verify_diagnostics_against_doc(&diagnostics, &doc_diags);
        }
    });
}
