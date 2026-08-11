use std::path::PathBuf;

use lsp_types::{DiagnosticSeverity, NumberOrString};
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::diag_on_line};

/// OLS03028: a method is only reachable through a model extension (_inherit) declared in a
/// module that is not a declared dependency ("indirect inheritance").
#[test]
fn test_ols03028_indirect_method_inheritance() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_indirect_b")
        .join("models")
        .join("models.py")
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);

    // Line 11: self.indirect_method() → only defined in module_indirect_a, which
    // module_indirect_b does not declare as a dependency.
    let on_line = diag_on_line(&diagnostics, 11);
    assert_eq!(
        on_line.len(),
        1,
        "expected exactly one diagnostic on line 12 (indirect_method call), got: {:?}",
        on_line
    );
    let diag = on_line[0];
    assert!(
        matches!(&diag.code, Some(NumberOrString::String(c)) if c == "OLS03028"),
        "expected OLS03028 on line 12, got: {:?}",
        diag.code
    );
    assert_eq!(diag.severity, Some(DiagnosticSeverity::WARNING), "OLS03028 should default to Warning");
    assert!(
        diag.message.contains("indirect_method") && diag.message.contains("module_indirect_a"),
        "expected message to mention the method and the missing module, got: {}",
        diag.message
    );

    // Line 10: self.own_method() → defined directly on this class, no dependency issue.
    let on_line = diag_on_line(&diagnostics, 10);
    assert!(
        on_line.is_empty(),
        "expected no diagnostics on line 11 (own_method call), got: {:?}",
        on_line
    );
}
