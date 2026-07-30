use std::path::{Path, PathBuf};

use lsp_types::NumberOrString;
use odoo_ls_server::constants::BuildSteps;
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::SourceFileKey;
use odoo_ls_server::odoo_version::OdooVersion;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::*;
use crate::test_utils::diag_on_line;

const ACCESS_LINE: u32 = 20;
const ACCESS_INVALID_FIELD_LINE: u32 = 23;
const ACCESS_INVALID_VALUE_LINE: u32 = 26;
const ACCESS_INVALID_FIELD_MULTI_CANDIDATE_LINE: u32 = 37;

fn revalidate(session: &mut SessionInfo, file_sym: SourceFileKey) {
    SymbolTable::invalidate_sub_functions(session, file_sym);
    BuildScheduler::queue(session, file_sym, BuildSteps::VALIDATION);
    BuildScheduler::process_rebuilds(session, false);
}

#[test]
fn test_ols03025_to_27_access_operator() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_for_diagnostics/models/access_operator.py")
        .sanitize();

    let file_sym = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
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

    // ('name', 'access', 'read') → 'name' is a Char field, not a Many2one/Id, so
    // exactly one OLS03027 should be raised
    let on_line = diag_on_line(&diagnostics, ACCESS_INVALID_FIELD_LINE);
    assert_eq!(
        on_line.len(),
        1,
        "expected exactly one diagnostic on line {} for an invalid access field, got: {:?}",
        ACCESS_INVALID_FIELD_LINE + 1,
        on_line
    );
    assert!(
        matches!(&on_line[0].code, Some(NumberOrString::String(c)) if c == "OLS03027"),
        "expected OLS03027 on line {} for an invalid access field, got: {:?}",
        ACCESS_INVALID_FIELD_LINE + 1,
        on_line[0].code
    );

    // ('shop_id', 'access', 'delete') → 'shop_id' is a valid Many2one, but 'delete'
    // is not one of read/write/create/unlink, so exactly one OLS03026 should be raised
    let on_line = diag_on_line(&diagnostics, ACCESS_INVALID_VALUE_LINE);
    assert_eq!(
        on_line.len(),
        1,
        "expected exactly one diagnostic on line {} for an invalid access value, got: {:?}",
        ACCESS_INVALID_VALUE_LINE + 1,
        on_line
    );
    assert!(
        matches!(&on_line[0].code, Some(NumberOrString::String(c)) if c == "OLS03026"),
        "expected OLS03026 on line {} for an invalid access value, got: {:?}",
        ACCESS_INVALID_VALUE_LINE + 1,
        on_line[0].code
    );

    // ('name', 'access', 'read') on a model where 'name' is redeclared by an
    // `_inherit`ing class: the field resolves to more than one candidate symbol.
    // Regression test: this must still raise exactly one OLS03027, not one per candidate.
    let on_line = diag_on_line(&diagnostics, ACCESS_INVALID_FIELD_MULTI_CANDIDATE_LINE);
    assert_eq!(
        on_line.len(),
        1,
        "expected exactly one diagnostic on line {} when the invalid field has multiple \
         resolved candidates (inherited override), got: {:?}",
        ACCESS_INVALID_FIELD_MULTI_CANDIDATE_LINE + 1,
        on_line
    );
    assert!(
        matches!(&on_line[0].code, Some(NumberOrString::String(c)) if c == "OLS03027"),
        "expected OLS03027 on line {} for an invalid access field with multiple candidates, got: {:?}",
        ACCESS_INVALID_FIELD_MULTI_CANDIDATE_LINE + 1,
        on_line[0].code
    );
}
