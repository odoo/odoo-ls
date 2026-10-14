mod setup;
mod test_utils;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::storage::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::{FunctionKey, SymbolKey};
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::Path;

const PROBE_FILE: &[&str] = &["odoo", "addons", "module_1", "models", "env_subscript_probe"];

fn returned_models(session: &mut SessionInfo, function: FunctionKey) -> Vec<String> {
    test_utils::resolve_symbol_types(session, function.into())
        .into_iter()
        .filter_map(|s| match s {
            SymbolKey::Class(c) => session.st()[c]._model.as_ref().map(|m| m.name.to_string()),
            _ => None,
        })
        .collect()
}

/// A function re-evaluated before `Environment.__getitem__` is rebuilt still resolves `env[model]`.
#[test]
fn test_env_subscript_after_getitem_invalidation() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let odoo_path = Path::new(&env::var("COMMUNITY_PATH").unwrap()).sanitize();

    let env_tree: &[&str] = if session.sync_odoo.version >= (18, 1) {
        &["odoo", "orm", "environments"]
    } else {
        &["odoo", "api"]
    };
    let env_file = session.sync_odoo.get_symbol(&odoo_path, (env_tree, &[]), u32::MAX)
        .first()
        .expect("expected the file declaring Environment")
        .as_source_file_key()
        .expect("Environment should be declared in a source file");
    let get_item = session.sync_odoo.get_symbol(&odoo_path, (env_tree, &["Environment", "__getitem__"]), u32::MAX)
        .first()
        .expect("expected Environment.__getitem__")
        .unwrap_function_key();
    let probe_file = session.sync_odoo.get_symbol(&odoo_path, (PROBE_FILE, &[]), u32::MAX)
        .first()
        .expect("expected the env_subscript_probe file")
        .as_source_file_key()
        .expect("env_subscript_probe should be a source file");
    let make_partner = session.sync_odoo.get_symbol(&odoo_path, (PROBE_FILE, &["EnvSubscriptProbe", "make_partner"]), u32::MAX)
        .first()
        .expect("expected EnvSubscriptProbe.make_partner")
        .unwrap_function_key();

    SyncOdoo::ensure_func_evaluations(&mut session, make_partner);
    assert_eq!(returned_models(&mut session, make_partner), vec!["res.partner"], "make_partner should return res.partner after the initial build");

    SymbolTable::invalidate_sub_functions(&mut session, env_file);
    SymbolTable::invalidate_sub_functions(&mut session, probe_file);
    assert!(session.st()[get_item].evaluations.is_empty(), "invalidating the Environment file should clear __getitem__");

    SyncOdoo::ensure_func_evaluations(&mut session, make_partner);
    assert_eq!(
        returned_models(&mut session, make_partner),
        vec!["res.partner"],
        "make_partner should still return res.partner when evaluated before __getitem__ is rebuilt"
    );
}
