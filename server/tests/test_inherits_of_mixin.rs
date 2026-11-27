mod setup;
mod test_utils;

use lsp_types::CompletionResponse;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::utils::{PathSanitizer, ToFilePath};
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
    // opened, as completion needs the AST of the file
    Odoo::handle_did_open(&mut session, lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri: FileMgr::pathname2uri(&test_file),
            language_id: "python".to_string(),
            version: 2,
            text: content.clone(),
        },
    });
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

    // completion at the end of `return self.<attribute>` in the method `method`
    let mut completion_labels = |method: &str, attribute: &str| -> Vec<String> {
        let needle = format!("return self.{attribute}");
        let (line, character) = find_position(&content, &format!("def {method}"), &needle);
        let response = CompletionFeature::autocomplete(&mut session, file_symbol, file_info, None, line, character + needle.len() as u32);
        let items = match response {
            Some(CompletionResponse::Array(items)) => items,
            Some(CompletionResponse::List(list)) => list.items,
            None => vec![],
        };
        items.into_iter().map(|i| i.label).collect()
    };

    // only fields are completed through `_inherits`
    let labels = completion_labels("use_target_value", "target_value");
    assert!(labels.iter().any(|l| l == "target_value"), "target_value should be completed through _inherits, got: {labels:?}");
    let labels = completion_labels("use_target_method", "target_method");
    assert!(!labels.iter().any(|l| l == "target_method"), "target_method should not be completed through _inherits, got: {labels:?}");

    // a model given by `_inherits` (through the mixin) then by `_inherit` is fully inherited
    let labels = completion_labels("use_inherited_target_method", "target_method");
    assert!(labels.iter().any(|l| l == "target_method"), "target_method should be completed through _inherit, got: {labels:?}");
}

/// In XML, `<field name="..."/>` resolves the fields given by `_inherits` (through a mixin, and recursively),
/// in views as in records. `<button name="..."/>` does not resolve methods given by `_inherits`.
#[test]
fn test_inherits_of_mixin_xml() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let module_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_1");
    let py_file = module_path.join("models").join("mixin_inherits_probe.py").sanitize();
    let xml_file = module_path.join("records").join("mixin_inherits_probe.xml").sanitize();
    let py_content = std::fs::read_to_string(&py_file).unwrap();
    let xml_content = std::fs::read_to_string(&xml_file).unwrap();
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let file_info = session.file_mgr().get_file_info(&xml_file).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&xml_file))
        .expect("Failed to get file symbol");

    // lines of the definitions in the python file, for the `name` attribute value `needle`, first occurrence after `after`
    let mut definition_lines = |after: &str, needle: &str| -> Vec<u32> {
        let (line, character) = find_position(&xml_content, after, needle);
        test_utils::get_definition_locs(&mut session, file_symbol, file_info, line, character + "name=\"".len() as u32 + 1)
            .into_iter()
            .filter(|l| l.target_uri.to_file_path().unwrap().sanitize() == py_file)
            .map(|l| l.target_range.start.line)
            .collect()
    };

    let (target_decl, _) = find_position(&py_content, "class InheritsTargetProbe", "target_value =");
    let lines = definition_lines("<form>", "name=\"target_value\"");
    assert!(lines.contains(&target_decl), "target_value in the view should lead to the field given by _inherits, got lines: {lines:?}");

    let (deep_decl, _) = find_position(&py_content, "class InheritsDeepProbe", "deep_value =");
    let lines = definition_lines("<form>", "name=\"deep_value\"");
    assert!(lines.contains(&deep_decl), "deep_value in the view should lead to the field given by the nested _inherits, got lines: {lines:?}");

    let (method_decl, _) = find_position(&py_content, "class InheritsTargetProbe", "def target_method");
    let lines = definition_lines("<form>", "name=\"target_method\"");
    assert!(!lines.contains(&method_decl), "target_method should not be visible through _inherits, got lines: {lines:?}");

    let (own_method_decl, _) = find_position(&py_content, "class InheritsConcreteProbe", "def use_target_value");
    let lines = definition_lines("<form>", "name=\"use_target_value\"");
    assert!(lines.contains(&own_method_decl), "use_target_value should lead to the method of the model, got lines: {lines:?}");

    let lines = definition_lines("id=\"inherits_concrete_probe_record\"", "name=\"target_value\"");
    assert!(lines.contains(&target_decl), "target_value in the record should lead to the field given by _inherits, got lines: {lines:?}");
}
