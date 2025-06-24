// Test the hover feature by calling get_hover on various symbols in the test addons.

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::PathBuf;

mod setup;

#[test]
fn test_hover_on_model_field_and_method() {
    // Setup server and session with test addons
    let mut odoo = setup::setup::setup_server(true);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    // Ensure the test file exists
    assert!(PathBuf::from(&test_file).exists(), "Test file does not exist: {}", test_file);
    let mut session = setup::setup::create_session(&mut odoo);

    // Get file symbol and file info
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    // Use get_file_info().symbol instead of get_file_symbol
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&test_file)
    ) else {
        panic!("Failed to get file symbol");
    };

    // Helper to get hover markdown string at a given (line, character)
    let mut get_hover_markdown = |line: u32, character: u32| {
        let hover = odoo_ls_server::features::hover::HoverFeature::get_hover(
            &mut session,
            &file_symbol,
            &file_info,
            line,
            character,
        );
        hover.and_then(|h| match h.contents {
            lsp_types::HoverContents::Markup(m) => Some(m.value),
            lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(s)) => Some(s),
            _ => None,
        })
    };

    // Hover on the model class name "BaseTestModel"
    let hover_model = get_hover_markdown(3, 6).unwrap_or_default();
    assert!(
        hover_model.contains("BaseTestModel"),
        "Hover on model class should show model name"
    );

    // Hover on the field "test_int"
    let hover_field = get_hover_markdown(8, 8).unwrap_or_default();
    assert!(
        hover_field.contains("test_int"),
        "Hover on field should show field name"
    );
    // This is not possible unless we load this as an odoo instance not custom entry point
    assert!(
        hover_field.contains("Integer"),
        "Hover on field should show field type"
    );

    // Hover on the method "get_test_int"
    let hover_method = get_hover_markdown(11, 8).unwrap_or_default();
    assert!(
         hover_method.contains("get_test_int"),
        "Hover on method should show method name"
    );

    assert!(
         hover_method.contains("(method) def get_test_int(self) -> int"),
        "Hover on `get_test_int` should show return type `int`"
    );

    // Hover on a reference to a constant (CONSTANT_1)
    let hover_const = get_hover_markdown(16, 23).unwrap_or_default();
    assert!(
        hover_const.contains("CONSTANT_1: int"),
        "Hover on constant should show constant name amd type int"
    );

    // Hover on onchange decorator
    let hover_onchange = get_hover_markdown(22, 22).unwrap_or_default();
    assert!(
        hover_onchange.contains("test_int: Integer"),
        "Hover on field_name in onchange should show field name and field type"
    );

    // Hover on depends decorator, on different sections
    let hover_partner_id = get_hover_markdown(26, 22).unwrap_or_default();
    assert!(
        hover_partner_id.contains("partner_id: Partner"),
        "Hover on field_name in depends should show field name and field type"
    );
    let hover_country_id = get_hover_markdown(26, 35).unwrap_or_default();
    assert!(
        hover_country_id.contains("country_id: Country"),
        "Hover on field_name in depends should show field name and field type"
    );
    let hover_code = get_hover_markdown(26, 43).unwrap_or_default();
    assert!(
        hover_code.contains("code: Char"), // TODO: Should it be str?
        "Hover on field_name in depends should show field name and field type"
    );

    //Hover on self.env with res.partner and test model name
    let hover_partner = get_hover_markdown(28, 24).unwrap_or_default();
    assert!(
        hover_partner.contains("Partner"),
        "Hover on self.env[\"res.partner\"] should show Partner model name"
    );
    let hover_test_class = get_hover_markdown(29, 24).unwrap_or_default();
    assert!(
        hover_test_class.contains("BaseTestModel"),
        "Hover on self.env[\"pygls.tests.base_test_model\"] should show Partner model name"
    );

    // Hover on a variable assignment (baseInstance1)
    let hover_var = get_hover_markdown(31, 0).unwrap_or_default();
    assert!(
        hover_var.contains("BaseTestModel"),
        "Hover on variable should show type info"
    );
}

// TODO:
// - Hover on domains
// - Hover on related fields parameters
// - Hover on computed field (compute method field)
