mod setup;
mod test_utils;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::SymbolKey;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::Path;

/// `cached_property`/`lazy_property`, bare or attribute-accessed, must be handled as properties.
/// Odoo declares `Environment.user`, `Environment.company`, ... with them.
#[test]
fn test_cached_and_lazy_property_are_properties() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/cached_property.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    let cases = [
        ("via_functools", true),
        ("via_cached_bare", true),
        ("via_lazy_attr", true),
        ("via_lazy_bare", true),
        ("plain_method", false),
    ];
    for (name, expected_property) in cases {
        let syms = session.sync_odoo.get_symbol(path.as_str(), (&[], &["Holder", name]), u32::MAX);
        assert!(!syms.is_empty(), "Holder.{name} should be found in the test file");
        let SymbolKey::Function(func_key) = syms[0] else {
            panic!("Holder.{name} should be a function symbol");
        };
        assert_eq!(
            session.st()[func_key].is_property, expected_property,
            "Holder.{name}: expected is_property = {expected_property}"
        );
    }

    // Hover presents a property, and not the signature of a method
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 26, 4)
        .unwrap_or_default();
    assert!(
        hover.contains("(property)") && !hover.contains("def via_functools"),
        "hover on h.via_functools should present it as a property, got: {hover:?}"
    );
}
