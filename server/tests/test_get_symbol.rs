// Test the hover feature by calling get_hover on various symbols in the test addons.

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::{PathSanitizer, ToFilePath};
use odoo_ls_server::Sy;
use odoo_ls_server::constants::OYarn;
use std::env;
use std::path::PathBuf;

mod setup;
mod test_utils;

#[test]
fn test_hover_on_model_field_and_method() {
    // Setup server and session with test addons
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    // Ensure the test file exists
    assert!(PathBuf::from(&test_file).exists(), "Test file does not exist: {}", test_file);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    // Use Lazy value for partner and country class names
    let partner_class_name = test_utils::PARTNER_CLASS_NAME(session.sync_odoo.full_version.as_str());
    let country_class_name = test_utils::COUNTRY_CLASS_NAME(session.sync_odoo.full_version.as_str());
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

    // Hover on the model class name "BaseTestModel"
    let hover_model = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 3, 6).unwrap_or_default();
    assert!(
        hover_model.contains("BaseTestModel"),
        "Hover on model class should show model name"
    );

    // Hover on the field "test_int"
    let hover_field = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 8, 8).unwrap_or_default();
    assert!(
        hover_field.contains("test_int"),
        "Hover on field should show field name"
    );
    // This is not possible unless we load this as an odoo instance not custom entry point
    assert!(
        hover_field.contains("Integer"),
        "Hover on field should show field type"
    );

    // Hover on related field "partner_company_phone_code"
    let hover_partner_id = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 10, 63).unwrap_or_default();
    assert!(
        hover_partner_id.contains(&format!("partner_id: {}", partner_class_name)),
        "Hover on field_name in related field name should show field name and field type"
    );
    let hover_country_id = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 10, 74).unwrap_or_default();
    assert!(
        hover_country_id.contains(&format!("country_id: {}", country_class_name)),
        "Hover on field_name in related field name should show field name and field type"
    );
    let hover_phone_code = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 10, 86).unwrap_or_default();
    assert!(
        hover_phone_code.contains("phone_code: int"),
        "Hover on field_name in related field name should show field name and field type"
    );

    // Hover on the method "get_test_int"
    let hover_method = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 14, 8).unwrap_or_default();
    assert!(
         hover_method.contains("get_test_int"),
        "Hover on method should show method name"
    );

    assert!(
         hover_method.contains("(method) def get_test_int(self) -> int"),
        "Hover on `get_test_int` should show return type `int`"
    );

    // Hover on a reference to a constant (CONSTANT_1)
    let hover_const = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 19, 23).unwrap_or_default();
    assert!(
        hover_const.contains("CONSTANT_1: int"),
        "Hover on constant should show constant name amd type int"
    );

    // Hover on onchange decorator
    let hover_onchange = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 25, 22).unwrap_or_default();
    assert!(
        hover_onchange.contains("test_int: int"),
        "Hover on field_name in onchange should show field name and field type"
    );

    // Hover on depends decorator, on different sections
    let hover_partner_id = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 29, 22).unwrap_or_default();
    assert!(
        hover_partner_id.contains(&format!("partner_id: {}", partner_class_name)),
        "Hover on field_name in depends should show field name and field type"
    );
    let hover_country_id = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 29, 35).unwrap_or_default();
    assert!(
        hover_country_id.contains(&format!("country_id: {}", country_class_name)),
        "Hover on field_name in depends should show field name and field type"
    );
    let hover_code = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 29, 43).unwrap_or_default();
    assert!(
        hover_code.contains("code: str"),
        "Hover on field_name in depends should show field name and field type"
    );

    //Hover on self.env with res.partner and test model name
    let hover_partner = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 31, 24).unwrap_or_default();
    assert!(
        hover_partner.contains("Partner"),
        "Hover on self.env[\"res.partner\"] should show Partner model name"
    );
    let hover_test_class = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 32, 24).unwrap_or_default();
    assert!(
        hover_test_class.contains("BaseTestModel"),
        "Hover on self.env[\"pygls.tests.base_test_model\"] should show Partner model name"
    );

    // Hover on domains, on different sections
    let hover_partner_id = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 33, 25).unwrap_or_default();
    assert!(
        hover_partner_id.contains(&format!("partner_id: {}", partner_class_name)),
        "Hover on field_name in search domain should show field name and field type"
    );
    let hover_country_id = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 33, 39).unwrap_or_default();
    assert!(
        hover_country_id.contains(&format!("country_id: {}", country_class_name)),
        "Hover on field_name in search domain should show field name and field type"
    );
    let hover_code = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 33, 48).unwrap_or_default();
    assert!(
        hover_code.contains("code: str"),
        "Hover on field_name in search domain should show field name and field type"
    );

    // Hover on a variable assignment (baseInstance1)
    let hover_var = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 35, 0).unwrap_or_default();
    assert!(
        hover_var.contains("BaseTestModel"),
        "Hover on variable should show type info"
    );
}

#[test]
fn test_definition() {
    // Setup server and session with test addons
    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = PathBuf::from(odoo_path).sanitize();
    let odoo_path = odoo_path.as_str();

    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let module1_test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    let module2_test_file = test_addons_path.join("module_2").join("models").join("base_test_models.py").sanitize();

    // Ensure the test file exists
    assert!(PathBuf::from(&module1_test_file).exists(), "Test file does not exist: {}", module1_test_file);
    assert!(PathBuf::from(&module2_test_file).exists(), "Test file does not exist: {}", module1_test_file);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    // Use Lazy value for partner and Country class names
    let partner_class_name = test_utils::PARTNER_CLASS_NAME(session.sync_odoo.full_version.as_str());
    let country_class_name = test_utils::COUNTRY_CLASS_NAME(session.sync_odoo.full_version.as_str());

    // Get file symbol and file info
    let file_mgr = session.sync_odoo.get_file_mgr();
    let m1_tf_file_info = file_mgr.borrow().get_file_info(&module1_test_file).unwrap();
    // Use get_file_info().symbol instead of get_file_symbol
    let Some(m1_tf_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&module1_test_file)
    ) else {
        panic!("Failed to get file symbol");
    };

    let m2_tf_file_info = file_mgr.borrow().get_file_info(&module2_test_file).unwrap();
    // Use get_file_info().symbol instead of get_file_symbol
    let Some(m2_tf_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&module2_test_file)
    ) else {
        panic!("Failed to get file symbol");
    };

    // Test definition for model class BaseTestModel compute something
    let compute_arg_locs = test_utils::get_definition_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 8, 50);
    assert_eq!(compute_arg_locs.len(), 1, "Expected 1 location for compute method '_compute_something'");
    assert_eq!(compute_arg_locs[0].target_uri.to_file_path().unwrap().sanitize(), module1_test_file, "Expected location to be in the same file");
    let sym_compute_something = m1_tf_file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel"), Sy!("_compute_something")]), u32::MAX);
    assert_eq!(sym_compute_something.len(), 1, "Expected 1 symbol for _compute_something");
    assert_eq!(file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_compute_something[0].borrow().range()), compute_arg_locs[0].target_range, "Expected _compute_something to be at the same location as the compute argument");

    // Test definition for model class BaseTestModel compute something in module_2, first on the super call
    let compute_arg_locs = test_utils::get_definition_locs(&mut session, &m2_tf_file_symbol, &m2_tf_file_info, 10, 36);
    assert_eq!(compute_arg_locs.len(), 1, "Expected 1 location for compute method '_compute_something'");
    assert_eq!(compute_arg_locs[0].target_uri.to_file_path().unwrap().sanitize(), module1_test_file, "Expected location to be in module_1 file");
    assert_eq!(file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_compute_something[0].borrow().range()), compute_arg_locs[0].target_range, "Expected _compute_something to be at the same location as the compute argument");

    // Then on the compute keyword argument in module_2, it should point to both methods in module_1 and module_2
    let compute_kwarg_locs = test_utils::get_definition_locs(&mut session, &m2_tf_file_symbol, &m2_tf_file_info, 6, 50);
    assert_eq!(compute_kwarg_locs.len(), 2, "Expected 2 locations for compute method '_compute_something'");
    assert!(compute_kwarg_locs.iter().any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == module1_test_file), "Expected one location to be in module_1 file");
    assert!(compute_kwarg_locs.iter().any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == module2_test_file), "Expected one location to be in module_2 file");
    let sym_compute_something_m2 = m2_tf_file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel"), Sy!("_compute_something")]), u32::MAX);
    assert_eq!(sym_compute_something_m2.len(), 1, "Expected 1 symbol for _compute_something in module_2");

    // Check that compute_kwarg_locs contains the range of the compute something syms from both files
    assert!(compute_kwarg_locs.iter().any(|loc| file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_compute_something[0].borrow().range()) == loc.target_range), "Expected _compute_something to be at the same location as the compute keyword argument in module_1");
    assert!(compute_kwarg_locs.iter().any(|loc| file_mgr.borrow().text_range_to_range(&mut session, &module2_test_file, sym_compute_something_m2[0].borrow().range()) == loc.target_range), "Expected _compute_something to be at the same location as the compute keyword argument in module_2");

    // Now test go to def of `partner_id.country_id.phone_code` on each field.
    let partner_id_locs = test_utils::get_definition_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 33, 25);
    assert_eq!(partner_id_locs.len(), 1, "Expected 1 location for partner_id");
    assert_eq!(partner_id_locs[0].target_uri.to_file_path().unwrap().sanitize(), module1_test_file, "Expected location to be in the same file");
    let sym_partner_id = m1_tf_file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel"), Sy!("partner_id")]), u32::MAX);
    assert_eq!(sym_partner_id.len(), 1, "Expected 1 symbol for partner_id");
    assert_eq!(file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_partner_id[0].borrow().range()), partner_id_locs[0].target_range, "Expected partner_id to be at the same location as the field");

    let country_id_locs = test_utils::get_definition_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 10, 74);
    let country_id_field_sym = session.sync_odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("res_partner")], vec![Sy!(partner_class_name), Sy!("country_id")]), u32::MAX);
    assert_eq!(country_id_field_sym.len(), 1, "Expected 1 location for country_id");
    let country_id_field_sym = country_id_field_sym[0].clone();
    let country_id_file = country_id_field_sym.borrow().get_file().unwrap().upgrade().unwrap().borrow().paths()[0].clone();
    assert_eq!(country_id_locs[0].target_uri.to_file_path().unwrap().sanitize(), country_id_file);
    // check that one of the country_id_locs is the same as the country_id field symbol
    assert!(country_id_locs.iter().any(|loc| loc.target_range == file_mgr.borrow().text_range_to_range(&mut session, &country_id_file, country_id_field_sym.borrow().range())), "Expected country_id to be at the same location as the field");

    // now the same for phone_code
    let phone_code_locs = test_utils::get_definition_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 10, 86);
    let phone_code_field_sym = session.sync_odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("res_country")], vec![Sy!(country_class_name), Sy!("phone_code")]), u32::MAX);
    assert_eq!(phone_code_field_sym.len(), 1, "Expected 1 location for phone_code");
    let phone_code_field_sym = phone_code_field_sym[0].clone();
    let phone_code_file = phone_code_field_sym.borrow().get_file().unwrap().upgrade().unwrap().borrow().paths()[0].clone();
    assert_eq!(phone_code_locs[0].target_uri.to_file_path().unwrap().sanitize(), phone_code_file);
    // check that one of the phone_code_locs is the same as the phone_code field
    assert!(phone_code_locs.iter().any(|loc| loc.target_range == file_mgr.borrow().text_range_to_range(&mut session, &phone_code_file, phone_code_field_sym.borrow().range())), "Expected phone_code to be at the same location as the field");
}
