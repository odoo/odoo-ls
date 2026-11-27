mod setup;
mod test_utils;

use lsp_types::CompletionResponse;
use odoo_ls_server::constants::BuildSteps;
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::file_mgr::FileInfoKey;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::KeyValidator;
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

    let file_info = session.sync_odoo.file_mgr.get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    // `        self.` in `Test.__init__`, where the attributes are assigned
    let completions = labels(CompletionFeature::autocomplete(&mut session, file_symbol, file_info, None, 5, 13));
    assert!(completions.iter().any(|l| l == "value"), "Expected 'value' after 'self.', got: {:?}", completions);
    assert!(completions.iter().any(|l| l == "count"), "Expected 'count' after 'self.', got: {:?}", completions);

    // `        return self.cou` in another method of the class
    let completions = labels(CompletionFeature::autocomplete(&mut session, file_symbol, file_info, None, 8, 23));
    assert!(completions.iter().any(|l| l == "count"), "Expected 'count' in another method, got: {:?}", completions);

    // `t.count` at module level is evaluated from the assignment in `__init__`
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, file_info, 21, 3).unwrap_or_default();
    assert!(hover.contains("count") && hover.contains("int"), "hover on t.count should show an int, got: {hover:?}");

    // a declared class attribute takes precedence over the instance attribute
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, file_info, 18, 21).unwrap_or_default();
    assert!(hover.contains("str") && !hover.contains("int"), "hover on self.label should show the class attribute, got: {hover:?}");

    // `t.extra` at module level is injected by the module-level `t.extra = "b"` (file ARCH_EVAL)
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, file_info, 24, 3).unwrap_or_default();
    assert!(hover.contains("extra") && hover.contains("str"), "hover on t.extra should show a str, got: {hover:?}");
}

/// Invalidating a file removes the ext symbols injected by its methods when the invalidation is
/// applied, at the end of the build phase: not while something is being built, as the build may use them.
#[test]
fn test_ext_symbols_removed_after_the_build() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/instance_attributes.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");
    let test_class = session.st().get_symbol(file_symbol.into(), (&[], &["Test"]), u32::MAX)[0];
    let count = session.st().get_ext_symbol(test_class, "count");
    assert_eq!(count.len(), 1, "`self.count` should be injected into Test");

    BuildScheduler::defer_invalidations(&mut session, |session| {
        SymbolTable::invalidate(session, file_symbol, BuildSteps::VALIDATION);
        assert_eq!(session.st().get_ext_symbol(test_class, "count"), count, "kept while something is being built");
    });
    SymbolTable::apply_pending_invalidations(&mut session);
    assert!(session.st().get_ext_symbol(test_class, "count").is_empty());
    assert!(!session.st().is_key_valid(count[0]));

    BuildScheduler::process_rebuilds(&mut session, false);
    assert_eq!(session.st().get_ext_symbol(test_class, "count").len(), 1, "`self.count` should be injected again");
}

/// `base.x = ...` also injects `x` into the functions, files and packages `base` evaluates to,
/// not only into classes (like `odoo._ = _` in `odoo/init.py`, on the `odoo` namespace).
#[test]
fn test_ext_symbols_on_non_class_targets() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/ext_targets.py")
        .sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    let file_info = session.sync_odoo.file_mgr.get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    // `func.flag` is injected into the function
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, file_info, 7, 6).unwrap_or_default();
    assert!(hover.contains("flag") && hover.contains("int"), "hover on func.flag should show an int, got: {hover:?}");

    // `json.custom` is injected into the (external) package
    let hover = test_utils::get_hover_markdown(&mut session, file_symbol, file_info, 10, 7).unwrap_or_default();
    assert!(hover.contains("custom") && hover.contains("str"), "hover on json.custom should show a str, got: {hover:?}");
}

fn definition_lines(session: &mut odoo_ls_server::threads::SessionInfo, file_symbol: odoo_ls_server::core::symbols::symbol_keys::SourceFileKey,
    file_info: FileInfoKey, line: u32, character: u32) -> Vec<(String, u32)> {
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
    let file_info = session.sync_odoo.file_mgr.get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // `paths.memo`: assigned in `AssetPaths.__init__`. Invalidating the validation of its file puts its
    // methods back as never evaluated (with their ext symbols removed), as the rebuild done before a
    // request has to evaluate them.
    let odoo_path = Path::new(&env::var("COMMUNITY_PATH").unwrap()).sanitize();
    let asset_paths = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "base", "models", "ir_asset"], &["AssetPaths"]), u32::MAX);
    assert_eq!(asset_paths.len(), 1, "AssetPaths not found");
    let asset_paths = asset_paths[0];
    let ir_asset_file = session.st().get_file(asset_paths).unwrap();
    SymbolTable::invalidate(&mut session, ir_asset_file, BuildSteps::VALIDATION);
    assert!(session.st().get_ext_symbol(asset_paths, "memo").is_empty());
    // as done before a goto definition request
    BuildScheduler::process_rebuilds(&mut session, true);
    let ir_asset = std::fs::read_to_string(Path::new(&env::var("COMMUNITY_PATH").unwrap()).join("odoo/addons/base/models/ir_asset.py")).unwrap();
    let class_start = ir_asset.lines().position(|l| l.starts_with("class AssetPaths")).unwrap();
    let class_end = class_start + 1 + ir_asset.lines().skip(class_start + 1).position(|l| l.starts_with("class ")).unwrap_or(usize::MAX - class_start - 1);
    let expected: Vec<(String, u32)> = ir_asset.lines().enumerate()
        .filter(|(i, l)| (class_start..class_end).contains(i) && l.trim_start().starts_with("self.memo ="))
        .map(|(i, _)| ("ir_asset.py".to_string(), i as u32))
        .collect();
    assert!(!expected.is_empty(), "the test expects an assignment of AssetPaths.memo");
    assert_eq!(definition_lines(&mut session, file_symbol, file_info, 9, 16), expected);

    // `self.cache` in another method of the model
    assert_eq!(definition_lines(&mut session, file_symbol, file_info, 13, 22), vec![("instance_attr_probe.py".to_string(), 10)]);
    // on the assignment itself
    assert_eq!(definition_lines(&mut session, file_symbol, file_info, 10, 14), vec![("instance_attr_probe.py".to_string(), 10)]);
}
