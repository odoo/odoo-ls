use std::path::{Path, PathBuf};

use lsp_types::NumberOrString;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::*;
use crate::test_utils::{diag_on_line, revalidate};

/// Test OLS03009 on the operators of a list domain that depend on the Odoo version.
/// `not =like` and `not =ilike` exist from saas-18.2.
/// `any!` and `not any!` are only accepted in `Domain(...)` conditions, never in a list.
#[test]
fn test_ols03009_version_operators() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_for_diagnostics/models/domain_operators.py")
        .sanitize();
    let file_sym = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("file symbol for domain_operators.py");
    let lines: Vec<String> = std::fs::read_to_string(&path).unwrap().lines().map(str::to_string).collect();
    // The line of the leaf using `operator`
    let line_of = |operator: &str| {
        lines.iter().position(|line| line.contains(&format!("'{operator}'"))).unwrap() as u32
    };

    // drain initial diagnostics so the next process_rebuilds publishes a clean set
    let _ = get_diagnostics_for_path(&mut session, &path);

    // (version, operators, whether they get OLS03009)
    let cases = [
        (OdooVersion::new(18, 1, 0), ["not =like", "not =ilike"], true),
        (OdooVersion::new(18, 2, 0), ["not =like", "not =ilike"], false),
        (OdooVersion::new(19, 0, 0), ["any!", "not any!"], true),
    ];
    let mut failures = vec![];
    for (version, operators, invalid) in cases {
        session.sync_odoo.version = version;
        revalidate(&mut session, file_sym);
        let diagnostics = get_diagnostics_for_path(&mut session, &path);
        for operator in operators {
            let on_line = diag_on_line(&diagnostics, line_of(operator));
            let as_expected = if invalid {
                on_line.iter().any(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "OLS03009"))
            } else {
                on_line.is_empty()
            };
            if !as_expected {
                failures.push(format!("{operator} on {version:?}: expected OLS03009: {invalid}, got {on_line:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "version operators:\n{}", failures.join("\n"));
}
