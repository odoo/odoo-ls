use std::path::PathBuf;

use lsp_types::NumberOrString;
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::diag_on_line};

/// `create_uid` and `write_uid` are added to every model as many2one to `res.users`: a domain path
/// goes through them to the fields of `res.users`. Each case is a call of `magic_fields.py`, found by
/// its line, with the codes expected on it.
#[test]
fn test_magic_fields_comodel() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_for_diagnostics/models/magic_fields.py")
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let lines: Vec<String> = std::fs::read_to_string(&path).unwrap().lines().map(str::to_string).collect();
    let cases: &[(&str, &[&str])] = &[
        ("self.search([('create_uid.login', '=', 'x')])", &[]),
        ("self.search([('write_uid.partner_id.name', '=', 'x')])", &[]),
        // A field of `res.users` that does not exist
        ("self.search([('create_uid.unknown_user_field', '=', 'x')])", &["OLS03011"]),
    ];
    let mut failures = vec![];
    for &(statement, expected) in cases {
        let line = lines.iter().position(|text| text.trim() == statement).unwrap_or_else(|| panic!("no line `{statement}`"));
        let codes: Vec<String> = diag_on_line(&diagnostics, line as u32).iter()
            .map(|diagnostic| match &diagnostic.code {
                Some(NumberOrString::String(code)) => code.clone(),
                other => format!("{other:?}"),
            })
            .collect();
        if codes != expected {
            failures.push(format!("{statement}: expected {expected:?}, got {codes:?}"));
        }
    }
    assert!(failures.is_empty(), "magic fields:\n{}", failures.join("\n"));
}
