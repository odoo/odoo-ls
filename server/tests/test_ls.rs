

use std::path::Path;
use std::env;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

#[test]
fn test_structure() {
    /* First, let's launch the server. It will setup a SyncOdoo struct, with a SyncChannel, that we can use to get the messages that the client would receive. */
    let (mut odoo, config) = setup::setup::setup_server(true);
    let _ = setup::setup::create_init_session(&mut odoo, config);
    let st = &odoo.symbol_table;

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();
    let odoo_path = odoo_path.as_str();
    assert!(!odoo.get_symbol(odoo_path, (&["odoo"], &[]), u32::MAX).is_empty());
    assert!(!odoo.get_symbol(odoo_path, (&["odoo", "addons"], &[]), u32::MAX).is_empty());
    assert!(!odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1"], &[]), u32::MAX).is_empty());
    assert!(!odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_2"], &[]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, (&["odoo", "addons", "not_a_module"], &[]), u32::MAX).is_empty());

    assert!(odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "not_loaded"], &[]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "not_loaded", "not_loaded_file"], &[]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "not_loaded", "not_loaded_file"], &["NotLoadedClass"]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "not_loaded", "not_loaded_file"], &["NotLoadedFunc"]), u32::MAX).is_empty());

    let models = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "models"], &[]), u32::MAX);
    assert!(models.len() == 1);
    assert!(st.get_symbol(models[0], (&["base_test_models"], &[]), u32::MAX).len() == 1);
    assert!(st.get_symbol(models[0], (&[], &["base_test_models"]), u32::MAX).len() == 1);
    assert!(st.get_symbol(models[0], (&["base_test_models"], &[]), u32::MAX)[0] !=
            st.get_symbol(models[0], (&[], &["base_test_models"]), u32::MAX)[0]);
    let module_1 = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1"], &[]), u32::MAX);
    assert!(module_1.len() == 1);
    test_imports(&odoo);
}

fn test_imports(odoo: &SyncOdoo) {
    //test direct imports
    let st = &odoo.symbol_table;
    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();
    let odoo_path = odoo_path.as_str();
    let model_var = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1"], &["models"]), u32::MAX);
    let model_dir = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "models"], &[]), u32::MAX);
    assert!(model_var.len() == 1);
    assert!(model_dir.len() == 1);
    assert!(model_dir[0] != model_var[0]);
    assert!(st.evaluations(model_var[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(model_var[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(st) == Some(model_dir[0]));
    let data_var = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1"], &["data"]), u32::MAX);
    assert!(data_var.len() == 1);
    assert!(st.evaluations(data_var[0]).as_ref().unwrap().is_empty());

    //test * imports
    let constants_dir = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "constants"], &[]), u32::MAX);
    assert!(constants_dir.len() == 1);
    let constants_dir = constants_dir[0];
    assert!(st.all_symbols(constants_dir).len() == 3);
    assert!(st.get_symbol(constants_dir, (&[], &["CONSTANT_1"]), u32::MAX).len() == 1);
    assert!(st.get_symbol(constants_dir, (&[], &["CONSTANT_2"]), u32::MAX).len() == 1);
    assert!(st.get_symbol(constants_dir, (&[], &["CONSTANT_3"]), u32::MAX).is_empty());
    assert!(st.get_symbol(constants_dir, (&["data"], &[]), u32::MAX).len() == 1);
    assert!(st.evaluations(st.get_symbol(constants_dir, (&[], &["CONSTANT_1"]), u32::MAX)[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(st.get_symbol(constants_dir, (&[], &["CONSTANT_1"]), u32::MAX)[0]).as_ref().unwrap()[0].value.is_none());
    assert!(st.evaluations(st.get_symbol(constants_dir, (&[], &["CONSTANT_2"]), u32::MAX)[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(st.get_symbol(constants_dir, (&[], &["CONSTANT_2"]), u32::MAX)[0]).as_ref().unwrap()[0].value.is_none());
    let data_dir = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "constants", "data"], &[]), u32::MAX);
    assert!(data_dir.len() == 1);
    let data_dir = data_dir[0];
    assert!(st.all_symbols(data_dir).len() == 4);
    assert!(st.get_symbol(data_dir, (&[], &["CONSTANT_1"]), u32::MAX).len() == 1);
    assert!(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), u32::MAX).len() == 1);
    assert!(st.get_symbol(data_dir, (&[], &["CONSTANT_3"]), u32::MAX).is_empty());
    assert!(st.get_symbol(data_dir, (&["constants"], &[]), u32::MAX).len() == 1);
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_1"]), u32::MAX)[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_1"]), u32::MAX)[0]).as_ref().unwrap()[0].value.is_none());
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), u32::MAX)[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), u32::MAX)[0]).as_ref().unwrap()[0].value.is_some());
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), u32::MAX)[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), u32::MAX)[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 22);
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), 26)[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), 26)[0]).as_ref().unwrap()[0].value.is_none());
    assert!(!st.evaluations(st.get_symbol(data_dir, (&[], &["CONSTANT_2"]), 26)[0]).as_ref().unwrap()[0].symbol.get_weak().weak.is_expired(st));

    //Test odoo.addons import
    let constant_1_var = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "models", "base_test_models"], &["CONSTANT_1"]), u32::MAX);
    assert!(constant_1_var.len() == 1);
    assert!(st.evaluations(constant_1_var[0]).as_ref().unwrap().len() == 1);
    let constant_1_var_data = odoo.get_symbol(odoo_path, (&["odoo", "addons", "module_1", "constants"], &["CONSTANT_1"]), u32::MAX);
    assert!(constant_1_var_data.len() == 1);
    assert!(st.evaluations(constant_1_var[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(st) == Some(constant_1_var_data[0]));

}
