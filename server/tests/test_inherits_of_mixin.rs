mod setup;
mod test_utils;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;
use std::path::Path;

/// Returns the (line, character) of the first occurrence of `needle` in `content`, after `after`
fn find_position(content: &str, after: &str, needle: &str) -> (u32, u32) {
    let start = content.find(after).unwrap_or_else(|| panic!("`{after}` not found in test file"));
    let offset = start + content[start..].find(needle).unwrap_or_else(|| panic!("`{needle}` not found after `{after}`"));
    let line = content[..offset].matches('\n').count() as u32;
    let line_start = content[..offset].rfind('\n').map_or(0, |i| i + 1);
    (line, (offset - line_start) as u32)
}

/// The fields given by the `_inherits` of a mixin are members of the models inheriting from the mixin,
/// including the fields given recursively by the `_inherits` of the inherits models. Methods are not.
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
    let content = std::fs::read_to_string(&test_file).unwrap();
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_info = session.file_mgr().get_file_info(&test_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file))
        .expect("Failed to get file symbol");

    // position on the attribute of `self.<attribute>`, first occurrence in the method `method`
    let mut definition_lines = |method: &str, attribute: &str| -> Vec<u32> {
        let (line, character) = find_position(&content, &format!("def {method}"), &format!("self.{attribute}"));
        test_utils::get_definition_locs(&mut session, file_symbol, file_info, line, character + "self.".len() as u32 + 1)
            .into_iter()
            .map(|l| l.target_range.start.line)
            .collect()
    };

    // field given by the `_inherits` of the mixin
    let (target_decl, _) = find_position(&content, "class InheritsTargetProbe", "target_value =");
    let lines = definition_lines("use_target_value", "target_value");
    assert!(
        lines.contains(&target_decl),
        "self.target_value should lead to the field given by the _inherits of the mixin, got lines: {lines:?}"
    );

    // field given by the `_inherits` of the `_inherits` target of the mixin
    let (deep_decl, _) = find_position(&content, "class InheritsDeepProbe", "deep_value =");
    let lines = definition_lines("use_deep_value", "deep_value");
    assert!(
        lines.contains(&deep_decl),
        "self.deep_value should lead to the field given by the nested _inherits, got lines: {lines:?}"
    );

    // methods are not visible through `_inherits`
    let (method_decl, _) = find_position(&content, "class InheritsTargetProbe", "def target_method");
    let lines = definition_lines("use_target_method", "target_method");
    assert!(
        !lines.contains(&method_decl),
        "self.target_method should not be visible through _inherits, got lines: {lines:?}"
    );
}
