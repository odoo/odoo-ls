mod setup;
mod test_utils;

use std::path::Path;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use test_utils::position_after;

/// Labels offered with the cursor right after the single occurrence of `needle`.
fn labels_after(session: &mut SessionInfo, path: &str, content: &str, needle: &str) -> Vec<String> {
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(session, Path::new(path)).expect("probe file symbol");
    let position = position_after(content, needle);
    let items = match CompletionFeature::autocomplete(session, file_symbol, &file_info, None, position.line, position.character) {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|item| item.label).collect()
}

/// An xml_id string literal completes from the current module and its dependencies.
#[test]
fn test_xml_id_completion() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let probe = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("data").join("addons")
        .join("module_1").join("models").join("xml_id_probe.py")
        .sanitize();
    let content = std::fs::read_to_string(&probe).unwrap();

    // env.ref("module_1.") -> the xml_ids the current module declares
    let refs = labels_after(&mut session, &probe, &content, r#"self.env.ref("module_1."#);
    assert!(refs.contains(&"module_1.test_xml_test_record".to_string()), "env.ref completes the ids of the module, got: {refs:?}");

    // groups="module_1." -> none of them, as none is a res.groups record
    let own = labels_after(&mut session, &probe, &content, r#"module_restricted = fields.Char(groups="module_1."#);
    assert!(own.is_empty(), "groups= only completes res.groups records, got: {own:?}");

    // groups="base.group_" and has_group("base.group_") -> the res.groups records of base
    for needle in [r#"group_restricted = fields.Char(groups="base.group_"#, r#"has_group("base.group_"#] {
        let groups = labels_after(&mut session, &probe, &content, needle);
        assert!(groups.contains(&"base.group_user".to_string()), "{needle} completes the groups of base, got: {groups:?}");
    }
}
