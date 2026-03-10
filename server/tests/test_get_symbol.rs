// Test the hover feature by calling get_hover on various symbols in the test addons.

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::{PathSanitizer, ToFilePath};
use odoo_ls_server::Sy;
use odoo_ls_server::constants::OYarn;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;

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
    let hover_var = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 43, 0).unwrap_or_default();
    assert!(
        hover_var.contains("BaseTestModel"),
        "Hover on variable should show type info"
    );

    // Hover on a method returning a variable that is assigned to a relational field
    // To check that the descriptor is correctly resolved
    let hover_var = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 38, 10).unwrap_or_default();
    assert!(
        hover_var.contains("ResPartner"),
        "Hover on variable should show type info"
    );

}

#[test]
fn test_hover_inverse_name_o2m(){
    // Setup server and session with test addons
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("diagnostics.py").sanitize();
    // Ensure the test file exists
    assert!(PathBuf::from(&test_file).exists(), "Test file does not exist: {}", test_file);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
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
    let hover_var = test_utils::get_hover_markdown(&mut session, &file_symbol, &file_info, 9, 73).unwrap_or_default();
    assert!(
        hover_var.contains("(variable) diagnostics_id: ModelWithDiagnostics"),
        "Hover on inverse o2m field should show correct type info"
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
    let compute_arg_locs = test_utils::get_definition_py_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 8, 50);
    assert_eq!(compute_arg_locs.len(), 1, "Expected 1 location for compute method '_compute_something'");
    assert_eq!(compute_arg_locs[0].target_uri.to_file_path().unwrap().sanitize(), module1_test_file, "Expected location to be in the same file");
    let sym_compute_something = m1_tf_file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel"), Sy!("_compute_something")]), u32::MAX);
    assert_eq!(sym_compute_something.len(), 1, "Expected 1 symbol for _compute_something");
    assert_eq!(file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_compute_something[0].borrow().range()), compute_arg_locs[0].target_range, "Expected _compute_something to be at the same location as the compute argument");

    // Test definition for model class BaseTestModel compute something in module_2, first on the super call
    let compute_arg_locs = test_utils::get_definition_py_locs(&mut session, &m2_tf_file_symbol, &m2_tf_file_info, 10, 36);
    assert_eq!(compute_arg_locs.len(), 1, "Expected 1 location for compute method '_compute_something'");
    assert_eq!(compute_arg_locs[0].target_uri.to_file_path().unwrap().sanitize(), module1_test_file, "Expected location to be in module_1 file");
    assert_eq!(file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_compute_something[0].borrow().range()), compute_arg_locs[0].target_range, "Expected _compute_something to be at the same location as the compute argument");

    // Then on the compute keyword argument in module_2, it should point to both methods in module_1 and module_2
    let compute_kwarg_locs = test_utils::get_definition_py_locs(&mut session, &m2_tf_file_symbol, &m2_tf_file_info, 6, 50);
    assert_eq!(compute_kwarg_locs.len(), 2, "Expected 2 locations for compute method '_compute_something'");
    assert!(compute_kwarg_locs.iter().any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == module1_test_file), "Expected one location to be in module_1 file");
    assert!(compute_kwarg_locs.iter().any(|loc| loc.target_uri.to_file_path().unwrap().sanitize() == module2_test_file), "Expected one location to be in module_2 file");
    let sym_compute_something_m2 = m2_tf_file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel"), Sy!("_compute_something")]), u32::MAX);
    assert_eq!(sym_compute_something_m2.len(), 1, "Expected 1 symbol for _compute_something in module_2");

    // Check that compute_kwarg_locs contains the range of the compute something syms from both files
    assert!(compute_kwarg_locs.iter().any(|loc| file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_compute_something[0].borrow().range()) == loc.target_range), "Expected _compute_something to be at the same location as the compute keyword argument in module_1");
    assert!(compute_kwarg_locs.iter().any(|loc| file_mgr.borrow().text_range_to_range(&mut session, &module2_test_file, sym_compute_something_m2[0].borrow().range()) == loc.target_range), "Expected _compute_something to be at the same location as the compute keyword argument in module_2");

    // Now test go to def of `partner_id.country_id.phone_code` on each field.
    let partner_id_locs = test_utils::get_definition_py_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 33, 25);
    assert_eq!(partner_id_locs.len(), 1, "Expected 1 location for partner_id");
    assert_eq!(partner_id_locs[0].target_uri.to_file_path().unwrap().sanitize(), module1_test_file, "Expected location to be in the same file");
    let sym_partner_id = m1_tf_file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel"), Sy!("partner_id")]), u32::MAX);
    assert_eq!(sym_partner_id.len(), 1, "Expected 1 symbol for partner_id");
    assert_eq!(file_mgr.borrow().text_range_to_range(&mut session, &module1_test_file, sym_partner_id[0].borrow().range()), partner_id_locs[0].target_range, "Expected partner_id to be at the same location as the field");

    let country_id_locs = test_utils::get_definition_py_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 10, 74);
    let country_id_field_sym = session.sync_odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("res_partner")], vec![Sy!(partner_class_name), Sy!("country_id")]), u32::MAX);
    assert_eq!(country_id_field_sym.len(), 1, "Expected 1 location for country_id");
    let country_id_field_sym = country_id_field_sym[0].clone();
    let country_id_file = country_id_field_sym.borrow().get_file().unwrap().upgrade().unwrap().borrow().paths()[0].clone();
    assert_eq!(country_id_locs[0].target_uri.to_file_path().unwrap().sanitize(), country_id_file);
    // check that one of the country_id_locs is the same as the country_id field symbol
    assert!(country_id_locs.iter().any(|loc| loc.target_range == file_mgr.borrow().text_range_to_range(&mut session, &country_id_file, country_id_field_sym.borrow().range())), "Expected country_id to be at the same location as the field");

    // now the same for phone_code
    let phone_code_locs = test_utils::get_definition_py_locs(&mut session, &m1_tf_file_symbol, &m1_tf_file_info, 10, 86);
    let phone_code_field_sym = session.sync_odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("res_country")], vec![Sy!(country_class_name), Sy!("phone_code")]), u32::MAX);
    assert_eq!(phone_code_field_sym.len(), 1, "Expected 1 location for phone_code");
    let phone_code_field_sym = phone_code_field_sym[0].clone();
    let phone_code_file = phone_code_field_sym.borrow().get_file().unwrap().upgrade().unwrap().borrow().paths()[0].clone();
    assert_eq!(phone_code_locs[0].target_uri.to_file_path().unwrap().sanitize(), phone_code_file);
    // check that one of the phone_code_locs is the same as the phone_code field
    assert!(phone_code_locs.iter().any(|loc| loc.target_range == file_mgr.borrow().text_range_to_range(&mut session, &phone_code_file, phone_code_field_sym.borrow().range())), "Expected phone_code to be at the same location as the field");
}
#[test]
fn test_model_subscription() {
    // Setup: Get the symbol for BaseTestModel and verify its existence
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&test_file)
    ) else {
        panic!("Failed to get file symbol");
    };
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let base_test_model_sym = file_symbol.borrow().get_symbol(&(vec![], vec![Sy!("BaseTestModel")]), u32::MAX);
    assert_eq!(base_test_model_sym.len(), 1, "Expected 1 symbol for BaseTestModel");
    let resolved_syms = test_utils::get_resolved_symbols_at_position(&mut session, &file_symbol, &file_info, 34, 10);
    assert!(
        resolved_syms.iter().any(|sym| Rc::ptr_eq(sym, &base_test_model_sym[0])),
        "Resolving a subscript of a model should include the model symbol itself.
        Expected to find BaseTestModel symbol among resolved symbols of `partner = self.search([], limit=2)[-1:]`"
    )
}

/// Test that hover over XML symbols (model name, record id, field name, ref) shows
/// associated XML record blocks, and that Python hover over model classes and fields
/// also shows linked XML record blocks when ir.model / ir.model.fields data exist.
#[test]
fn test_xml_hover() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let xml_file_path = test_addons_path.join("module_1").join("records").join("test_records.xml").sanitize();
    let py_file_path = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    assert!(PathBuf::from(&xml_file_path).exists(), "XML test file does not exist: {}", xml_file_path);
    assert!(PathBuf::from(&py_file_path).exists(),  "Python test file does not exist: {}", py_file_path);

    // --- Obtain XML file symbol and file info ---
    // data_symbols are populated by load_data during create_init_session.
    let Some(xml_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&xml_file_path),
    ) else {
        panic!("Failed to get XML file symbol for {}", xml_file_path);
    };
    let file_mgr = session.sync_odoo.get_file_mgr();
    let xml_file_info = file_mgr.borrow().get_file_info(&xml_file_path)
        .unwrap_or_else(|| panic!("XML file info not found in file manager for {}", xml_file_path));

    // --- Obtain Python file symbol and file info ---
    let py_file_info = file_mgr.borrow().get_file_info(&py_file_path).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&py_file_path),
    ) else {
        panic!("Failed to get Python file symbol");
    };

    // ==============================
    // XML HOVER TESTS
    // ==============================

    // 1. Hover on the `id` attribute value of `xml_test_model` (line 2, char 20).
    //    `    <record id="xml_test_model" model="ir.model">`
    //    The id attr hover should emit an XML_DATA item for that record itself.
    let hover_id_irmodel = test_utils::get_hover_xml_markdown(
        &mut session, &xml_file_symbol, &xml_file_info, 2, 20,
    ).unwrap_or_else(|| panic!("XML hover on id attr of xml_test_model returned None"));
    assert!(
        hover_id_irmodel.contains("XML record"),
        "Hover on record id should show 'XML record'; got:\n{}", hover_id_irmodel
    );
    assert!(
        hover_id_irmodel.contains("xml_test_model"),
        "Hover on record id should show the xml-id; got:\n{}", hover_id_irmodel
    );
    assert!(
        hover_id_irmodel.contains("ir.model"),
        "Hover on ir.model record id should show model name 'ir.model'; got:\n{}", hover_id_irmodel
    );

    // 2. Hover on the `id` attribute value of `test_xml_test_record` (line 12, char 20).
    //    `    <record id="test_xml_test_record" model="pygls.tests.xml_test_model">`
    let hover_id_data = test_utils::get_hover_xml_markdown(
        &mut session, &xml_file_symbol, &xml_file_info, 12, 20,
    ).unwrap_or_else(|| panic!("XML hover on id attr of test_xml_test_record returned None"));
    assert!(
        hover_id_data.contains("XML record"),
        "Hover on data record id should show 'XML record'; got:\n{}", hover_id_data
    );
    assert!(
        hover_id_data.contains("test_xml_test_record"),
        "Hover on data record id should show the xml-id; got:\n{}", hover_id_data
    );
    assert!(
        hover_id_data.contains("pygls.tests.xml_test_model"),
        "Hover on data record id should show the model name; got:\n{}", hover_id_data
    );

    // 3. Hover on the `model` attribute value of `test_xml_test_record` (line 12, char 48).
    //    `    <record id="test_xml_test_record" model="pygls.tests.xml_test_model">`
    //    pygls.tests.xml_test_model exists only in XML (no Python class), so expected:
    //    ir.model XML record block only.
    let hover_model_attr = test_utils::get_hover_xml_markdown(
        &mut session, &xml_file_symbol, &xml_file_info, 12, 48,
    ).unwrap_or_else(|| panic!("XML hover on model attr returned None"));
    assert!(
        hover_model_attr.contains("XML record"),
        "Hover on model attr should show an XML record (ir.model) block; got:\n{}", hover_model_attr
    );
    assert!(
        hover_model_attr.contains("xml_test_model"),
        "Hover on model attr should reference the ir.model xml-id; got:\n{}", hover_model_attr
    );

    // 4. Hover on the `name` attribute value of the `partner_id` field (line 13, char 22).
    //    `        <field name="partner_id"/>`
    //    Expected: ir.model.fields XML record block.
    let hover_field_name = test_utils::get_hover_xml_markdown(
        &mut session, &xml_file_symbol, &xml_file_info, 13, 22,
    ).unwrap_or_else(|| panic!("XML hover on field name attr returned None"));
    assert!(
        hover_field_name.contains("partner_id"),
        "Hover on field name attr should show field name; got:\n{}", hover_field_name
    );
    assert!(
        hover_field_name.contains("XML record"),
        "Hover on field name attr should show an XML record (ir.model.fields) block; got:\n{}", hover_field_name
    );
    assert!(
        hover_field_name.contains("field_xml_test_model_partner_id"),
        "Hover on field name attr should reference the ir.model.fields xml-id; got:\n{}", hover_field_name
    );

    // ==============================
    // PYTHON HOVER TESTS
    // ==============================

    // 5. Hover on `self.env["pygls.tests.xml_test_model"]` (line 36, char 24).
    //    `        self.env["pygls.tests.xml_test_model"]`
    //    pygls.tests.xml_test_model is an XML-only model (no Python class).
    //    Expected: ir.model XML record block (xml_test_model).
    let hover_py_model_str = test_utils::get_hover_markdown(
        &mut session, &py_file_symbol, &py_file_info, 36, 24,
    ).unwrap_or_else(|| panic!("Python hover on model name string returned None"));
    assert!(
        hover_py_model_str.contains("XML record"),
        "Python hover on XML-only model string should show ir.model record block; got:\n{}", hover_py_model_str
    );
    assert!(
        hover_py_model_str.contains("test_records.xml"),
        "Python hover on XML-only model string should reference the ir.model xml-id; got:\n{}", hover_py_model_str
    );

    // 6. Hover on `self.env.ref("module_1.xml_test_model")` (line 35, char 33).
    //    `        self.env.ref("module_1.xml_test_model")`
    //    Expected: the xml_test_model XML record block.
    let hover_py_ref_str = test_utils::get_hover_markdown(
        &mut session, &py_file_symbol, &py_file_info, 35, 33,
    ).unwrap_or_else(|| panic!("Python hover on xml_id ref string returned None"));
    assert!(
        hover_py_ref_str.contains("XML record"),
        "Python hover on xml_id ref string should show the XML record block; got:\n{}", hover_py_ref_str
    );
    assert!(
        hover_py_ref_str.contains("xml_test_model"),
        "Python hover on xml_id ref string should reference the xml-id; got:\n{}", hover_py_ref_str
    );
}

/// Test go-to-definition for XML model name and ref attributes, and from Python model name strings.
#[test]
fn test_xml_definition() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let xml_file_path = test_addons_path.join("module_1").join("records").join("test_records.xml").sanitize();
    let py_file_path  = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();

    assert!(PathBuf::from(&xml_file_path).exists(), "XML file missing: {}", xml_file_path);
    assert!(PathBuf::from(&py_file_path).exists(),  "Python file missing: {}", py_file_path);

    let Some(xml_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session, &PathBuf::from(&xml_file_path),
    ) else {
        panic!("Failed to get XML file symbol for {}", xml_file_path);
    };
    let file_mgr = session.sync_odoo.get_file_mgr();
    let xml_file_info = file_mgr.borrow().get_file_info(&xml_file_path)
        .unwrap_or_else(|| panic!("XML file info not found: {}", xml_file_path));

    let py_file_info = file_mgr.borrow().get_file_info(&py_file_path).unwrap();
    let Some(py_file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session, &PathBuf::from(&py_file_path),
    ) else {
        panic!("Failed to get Python file symbol");
    };

    // ============================================================
    // 1. XML -> GoToDef on `model="pygls.tests.xml_test_model"`
    //    (line 12, char 48 inside the model attr value)
    //    pygls.tests.xml_test_model has no Python class, so expected:
    //    only the ir.model XML record (xml_test_model) in test_records.xml.
    // ============================================================
    let xml_model_attr_locs = test_utils::get_definition_xml_locs(
        &mut session, &xml_file_symbol, &xml_file_info, 12, 48,
    );
    assert!(!xml_model_attr_locs.is_empty(),
        "XML model attr: GoToDef should return at least one location");
    assert!(
        xml_model_attr_locs.iter().any(|loc|
            loc.target_uri.to_file_path().unwrap().sanitize() == xml_file_path
        ),
        "XML model attr: one location should be the ir.model record in test_records.xml; got: {:?}",
        xml_model_attr_locs
    );

    // ============================================================
    // 2. XML -> GoToDef on `ref="model_base_test_model"`
    //    (line 8, char 40 inside the ref attr value)
    // Expected: exactly the model_base_test_model record inside test_records.xml.
    // ============================================================
    let xml_ref_locs = test_utils::get_definition_xml_locs(
        &mut session, &xml_file_symbol, &xml_file_info, 8, 40,
    );
    assert_eq!(xml_ref_locs.len(), 1,
        "XML ref attr: GoToDef should return exactly one location (the record in the XML file); got: {:?}",
        xml_ref_locs
    );
    assert_eq!(
        xml_ref_locs[0].target_uri.to_file_path().unwrap().sanitize(), xml_file_path,
        "XML ref attr: GoToDef should point into test_records.xml"
    );

    // ============================================================
    // 3. Python -> GoToDef on `"pygls.tests.xml_test_model"`
    //    (line 35, char 24 inside the string in self.env["..."])
    //    pygls.tests.xml_test_model is an XML-only model — no Python class.
    //    Expected: the ir.model XML record (xml_test_model) in test_records.xml.
    // ============================================================
    let py_model_locs = test_utils::get_definition_py_locs(
        &mut session, &py_file_symbol, &py_file_info, 35, 24,
    );
    assert!(!py_model_locs.is_empty(),
        "Python model string: GoToDef should return at least one location");
    assert!(
        py_model_locs.iter().any(|loc|
            loc.target_uri.to_file_path().unwrap().sanitize() == xml_file_path
        ),
        "Python model string: one location should be the ir.model record in test_records.xml; got: {:?}",
        py_model_locs
    );

    // ============================================================
    // 4. Python -> GoToDef on `"module_1.xml_test_model"` (xml_id ref)
    //    base_test_models.py line 36; char 35  (0-indexed):
    //    `        self.env.ref("module_1.xml_test_model")`
    // Expected: the xml_test_model XML record in test_records.xml.
    // ============================================================
    let py_ref_locs = test_utils::get_definition_py_locs(
        &mut session, &py_file_symbol, &py_file_info, 36, 35,
    );
    assert!(!py_ref_locs.is_empty(),
        "Python xml_id ref string: GoToDef should return at least one location");
    assert!(
        py_ref_locs.iter().any(|loc|
            loc.target_uri.to_file_path().unwrap().sanitize() == xml_file_path
        ),
        "Python xml_id ref string: one location should be the xml_test_model record in test_records.xml; got: {:?}",
        py_ref_locs
    );
}