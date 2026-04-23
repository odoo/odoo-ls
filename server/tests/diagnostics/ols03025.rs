use std::path::PathBuf;

use lsp_types::NumberOrString;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::SourceFileKey;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::*;
use crate::test_utils::diag_on_line;

const ACCESS_LINE: u32 = 19;

fn revalidate(session: &mut SessionInfo, file_sym: SourceFileKey) {
    session.st_mut().invalidate_sub_functions(file_sym);
    session.sync_odoo.add_to_validations(file_sym);
    SyncOdoo::process_rebuilds(session, false);
}

#[test]
fn test_ols03025_access_operator_version_gated() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_for_diagnostics/models/access_operator.py")
        .sanitize();

    let file_sym = SyncOdoo::get_symbol_of_opened_file(&mut session, &PathBuf::from(&path))
        .expect("file symbol for access_operator.py");

    // drain initial diagnostics so the next process_rebuilds publishes a clean set
    let _ = get_diagnostics_for_path(&mut session, &path);

    // version < 19.3 → exactly one OLS03025 on the search line
    session.sync_odoo.version = OdooVersion::new(19, 2, 0);
    revalidate(&mut session, file_sym);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let on_line = diag_on_line(&diagnostics, ACCESS_LINE);
    assert_eq!(
        on_line.len(),
        1,
        "expected exactly one diagnostic on line {} for version 19.2, got: {:?}",
        ACCESS_LINE + 1,
        on_line
    );
    let only = on_line[0];
    assert!(
        matches!(&only.code, Some(NumberOrString::String(c)) if c == "OLS03025"),
        "expected OLS03025 on line {} for version 19.2, got: {:?}",
        ACCESS_LINE + 1,
        only.code
    );

    // version >= 19.3 → no diagnostic at all on that line: 'access' is a valid operator,
    // so no OLS03009 or OLS03025 should be raised
    session.sync_odoo.version = OdooVersion::new(19, 3, 0);
    revalidate(&mut session, file_sym);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let on_line = diag_on_line(&diagnostics, ACCESS_LINE);
    assert!(
        on_line.is_empty(),
        "expected no diagnostics on line {} for version 19.3 (neither OLS03025 nor OLS03009), got: {:?}",
        ACCESS_LINE + 1,
        on_line
    );
}
