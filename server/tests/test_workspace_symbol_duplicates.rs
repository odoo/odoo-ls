// Regression test for workspace/symbol returning duplicate results when several entry points
// share the same root symbol (e.g. the MAIN entry point and an ADDON entry point derived from
// an `addons_paths` entry). See EntryPointMgr::add_entry_to_addons / EntryPoint::new_with_shared_root.

use lsp_types::WorkspaceSymbolResponse;
use odoo_ls_server::features::workspace_symbols::WorkspaceSymbolFeature;

mod setup;

#[test]
fn test_workspace_symbol_no_duplicates_for_entry_points_sharing_root() {
    // setup_server(true) configures a MAIN entry point (COMMUNITY_PATH) plus an ADDON entry
    // point for the test addons path (via `addons_paths`). The ADDON entry point shares its
    // root symbol with the MAIN entry point, so `iter_all()` yields two entry points pointing
    // at the same tree.
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    // "Module2CustomModel" is defined exactly once in the test addons (module_2/models/base_test_models.py).
    let response = WorkspaceSymbolFeature::get_workspace_symbols(&mut session, "Module2CustomModel".to_string())
        .expect("workspace/symbol request should not error");
    let Some(WorkspaceSymbolResponse::Nested(symbols)) = response else {
        panic!("Expected a nested workspace symbol response");
    };

    let matches: Vec<_> = symbols.iter().filter(|s| s.name == "Module2CustomModel").collect();
    assert_eq!(
        matches.len(), 1,
        "Expected exactly 1 workspace/symbol result for a symbol with a single definition, got {}: {:?}. \
         Entry points sharing the same root (e.g. MAIN + an ADDON entry point from addons_paths) are each \
         fully traversed by get_workspace_symbols, producing duplicate results.",
        matches.len(), matches
    );
}
