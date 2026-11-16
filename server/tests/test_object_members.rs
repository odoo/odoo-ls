mod setup;
mod test_utils;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;
use std::path::Path;
use test_utils::get_resolved_symbols_at_position;

/// Members of `object` (`__class__`, ...) are found on models, even if `object` is not in their bases.
#[test]
fn test_object_members_on_model() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1")
        .join("models")
        .join("instance_attr_probe.py")
        .sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {test_file}");
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // `        InstanceAttrProbe.__class__` → on the class
    let resolved = get_resolved_symbols_at_position(&mut session, file_symbol, file_info, 16, 30);
    assert!(!resolved.is_empty(), "InstanceAttrProbe.__class__ should be resolved");

    // `        return self.__class__` → on a recordset
    let resolved = get_resolved_symbols_at_position(&mut session, file_symbol, file_info, 17, 25);
    assert!(!resolved.is_empty(), "self.__class__ should be resolved");
}
