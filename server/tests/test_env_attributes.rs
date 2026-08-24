mod setup;
mod test_utils;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::file_mgr::FileInfo;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::SourceFileKey;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::cell::RefCell;
use std::env;
use std::path::Path;
use std::rc::Rc;
use test_utils::get_resolved_symbols_at_position;

fn resolved_names(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Vec<String> {
    get_resolved_symbols_at_position(session, file_symbol, file_info, line, character)
        .iter()
        .map(|&s| session.st().name(s).to_string())
        .collect()
}

/// `self.env.user` / `company` / `companies` / `lang` must resolve to their real types, and an
/// attribute chain through them (`self.env.user.has_group(...)`) to the model's member.
#[test]
fn test_env_attributes_resolve() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path
        .join("module_1")
        .join("models")
        .join("env_attr_probe.py")
        .sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {test_file}");
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // self.env.user -> res.users (ResUsers on >=18.1, Users before)
    let user = resolved_names(&mut session, file_symbol, &file_info, 9, 18);
    assert!(!user.is_empty(), "self.env.user should resolve to the res.users class, got nothing");

    // self.env.user.has_group -> res.users method `has_group`, through the cached_property
    let has_group = resolved_names(&mut session, file_symbol, &file_info, 10, 23);
    assert!(
        has_group.iter().any(|n| n == "has_group"),
        "self.env.user.has_group should resolve to the res.users method has_group, got: {has_group:?}"
    );

    // self.env.company and self.env.companies -> res.company
    let company = resolved_names(&mut session, file_symbol, &file_info, 11, 18);
    let companies = resolved_names(&mut session, file_symbol, &file_info, 12, 18);
    assert!(!company.is_empty(), "self.env.company should resolve to the res.company class");
    assert_eq!(
        company, companies,
        "self.env.companies should resolve to the same class as self.env.company"
    );
    assert_ne!(
        user, company,
        "self.env.user and self.env.company should resolve to different model classes"
    );

    // self.env.lang -> str
    let lang = resolved_names(&mut session, file_symbol, &file_info, 13, 18);
    assert!(
        lang.iter().any(|n| n == "str"),
        "self.env.lang should resolve to str, got: {lang:?}"
    );
}

/// A property reached through a variable (`user = self.env.user; user.has_group(...)`) must resolve
/// just like the direct chain, by following the property mid-chain in `follow_ref`.
#[test]
fn test_env_user_through_variable_resolves() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path
        .join("module_1")
        .join("models")
        .join("env_attr_probe.py")
        .sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // (a) the `user` variable resolves to the res.users class, and not to the property function
    let user_var = resolved_names(&mut session, file_symbol, &file_info, 15, 8);
    assert!(
        !user_var.is_empty() && !user_var.iter().any(|n| n == "user"),
        "the `user` variable should resolve to the res.users class, got: {user_var:?}"
    );

    // (b) its hover type presents the model class, and not `Any`
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 15, 8)
        .unwrap_or_default();
    assert!(
        hover.contains(&format!("(variable) user: {}", user_var[0])) && !hover.contains(": Any"),
        "hover on the `user` variable should show its model type, got: {hover:?}"
    );

    // (c) `user.has_group` resolves to the res.users method through the variable
    let has_group = resolved_names(&mut session, file_symbol, &file_info, 16, 15);
    assert!(
        has_group.iter().any(|n| n == "has_group"),
        "user.has_group should resolve to the res.users method has_group, got: {has_group:?}"
    );

    // (d) completion on `user.` lists members of the resolved model
    let response = CompletionFeature::autocomplete(&mut session, file_symbol, &file_info, None, 16, 13);
    let labels: Vec<String> = match response {
        Some(CompletionResponse::Array(items)) => items.into_iter().map(|i| i.label).collect(),
        Some(CompletionResponse::List(list)) => list.items.into_iter().map(|i| i.label).collect(),
        None => vec![],
    };
    assert!(
        labels.iter().any(|l| l == "has_group"),
        "completion on `user.` should list res.users members such as has_group, got: {labels:?}"
    );
}
