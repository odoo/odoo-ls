mod setup;
mod test_utils;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::storage::SymbolTable;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::Path;

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let items = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => vec![],
    };
    items.into_iter().map(|i| i.label).collect()
}

/// `base.x = ...` injects `x` into the classes `base` evaluates to (`self` in a method, or any
/// instance), as an ext symbol created in ARCH_EVAL: it must be completed and evaluated on the class instances.
#[test]
fn test_instance_attributes() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/instance_attributes.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    // `        self.` in `Test.__init__`, where the attributes are assigned
    let completions = labels(CompletionFeature::autocomplete(&mut session, file_symbol, &file_info, None, 5, 13));
    assert!(completions.iter().any(|l| l == "value"), "Expected 'value' after 'self.', got: {:?}", completions);
    assert!(completions.iter().any(|l| l == "count"), "Expected 'count' after 'self.', got: {:?}", completions);

    // `        return self.cou` in another method of the class
    let completions = labels(CompletionFeature::autocomplete(&mut session, file_symbol, &file_info, None, 8, 23));
    assert!(completions.iter().any(|l| l == "count"), "Expected 'count' in another method, got: {:?}", completions);

    // `t.count` at module level is evaluated from the assignment in `__init__`
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 21, 3).unwrap_or_default();
    assert!(hover.contains("count") && hover.contains("int"), "hover on t.count should show an int, got: {hover:?}");

    // a declared class attribute takes precedence over the instance attribute
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 18, 21).unwrap_or_default();
    assert!(hover.contains("str") && !hover.contains("int"), "hover on self.label should show the class attribute, got: {hover:?}");

    // `t.extra` at module level is injected by the module-level `t.extra = "b"` (file ARCH_EVAL)
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, &file_info, 24, 3).unwrap_or_default();
    assert!(hover.contains("extra") && hover.contains("str"), "hover on t.extra should show a str, got: {hover:?}");
}

fn definition_lines(session: &mut odoo_ls_server::threads::SessionInfo, file_symbol: odoo_ls_server::core::symbols::symbol_keys::SourceFileKey,
    file_info: &std::rc::Rc<std::cell::RefCell<odoo_ls_server::core::file_mgr::FileInfo>>, line: u32, character: u32) -> Vec<(String, u32)> {
    let mut result: Vec<(String, u32)> = test_utils::get_definition_locs(session, file_symbol, file_info, line, character)
        .into_iter()
        .map(|l| (l.target_uri.path().as_str().rsplit('/').next().unwrap().to_string(), l.target_range.start.line))
        .collect();
    result.sort();
    result
}

/// Go to definition on an instance attribute leads to all its assignments, even when they are in
/// methods that were not evaluated yet (here in the base addon, that is not validated).
#[test]
fn test_instance_attributes_definition() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_1/models/instance_attr_probe.py")
        .sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // `paths.memo`: assigned in `AssetPaths.__init__`. Put its file back as if its methods were never
    // evaluated (they could have been, for their return values), as goto has to evaluate them.
    let odoo_path = Path::new(&env::var("COMMUNITY_PATH").unwrap()).sanitize();
    let asset_paths = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "base", "models", "ir_asset"], &["AssetPaths"]), u32::MAX);
    assert_eq!(asset_paths.len(), 1, "AssetPaths not found");
    let asset_paths = asset_paths[0];
    let ir_asset_file = session.st().get_file(asset_paths).unwrap();
    SymbolTable::invalidate_sub_functions(&mut session, ir_asset_file);
    for method in session.st().iter_inner_functions(asset_paths) {
        session.st_mut().remove_ext_symbols(method.into());
    }
    assert!(session.st().get_ext_symbol(asset_paths, "memo").is_empty());
    let ir_asset = std::fs::read_to_string(Path::new(&env::var("COMMUNITY_PATH").unwrap()).join("odoo/addons/base/models/ir_asset.py")).unwrap();
    let class_start = ir_asset.lines().position(|l| l.starts_with("class AssetPaths")).unwrap();
    let class_end = class_start + 1 + ir_asset.lines().skip(class_start + 1).position(|l| l.starts_with("class ")).unwrap_or(usize::MAX - class_start - 1);
    let expected: Vec<(String, u32)> = ir_asset.lines().enumerate()
        .filter(|(i, l)| (class_start..class_end).contains(i) && l.trim_start().starts_with("self.memo ="))
        .map(|(i, _)| ("ir_asset.py".to_string(), i as u32))
        .collect();
    assert!(!expected.is_empty(), "the test expects an assignment of AssetPaths.memo");
    assert_eq!(definition_lines(&mut session, file_symbol, &file_info, 9, 16), expected);

    // `self.cache` in another method of the model
    assert_eq!(definition_lines(&mut session, file_symbol, &file_info, 13, 22), vec![("instance_attr_probe.py".to_string(), 10)]);
    // on the assignment itself
    assert_eq!(definition_lines(&mut session, file_symbol, &file_info, 10, 14), vec![("instance_attr_probe.py".to_string(), 10)]);
}
