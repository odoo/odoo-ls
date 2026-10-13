mod setup;
mod test_utils;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;
use std::path::Path;

/// The fields given by the `_inherits` of a mixin are members of the models inheriting from the mixin.
#[test]
fn test_inherits_of_mixin() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1")
        .join("models")
        .join("mixin_inherits_probe.py")
        .sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {test_file}");
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // `        return self.target_value` → field given by the `_inherits` of the mixin, declared line 8
    let lines: Vec<u32> = test_utils::get_definition_locs(&mut session, file_symbol, file_info, 25, 25)
        .into_iter()
        .map(|l| l.target_range.start.line)
        .collect();
    assert!(
        lines.contains(&7),
        "self.target_value should lead to the field given by the _inherits of the mixin, got lines: {lines:?}"
    );
}
