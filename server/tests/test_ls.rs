

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
    //assert!(compare_symbol_with_json(module_1, "tests/module_1_structure.json"))
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

/*
fn compare_symbol_with_json(symbol: Rc<RefCell<Symbol>>, json_path: &str) -> bool {
    let file = File::open(json_path).expect("File not found");
    let reader = BufReader::new(file);
    let json: Value = serde_json::from_reader(reader).unwrap();
    _test_symbol_with_json_value(symbol, json)
}

fn _test_symbol_with_json_value(symbol: Rc<RefCell<Symbol>>, json: Value) -> bool {
    //Keep subsymbol to test byafter and the corresponding json value
    let mut module_symbols: Vec<(Rc<RefCell<Symbol>>, Value)> = vec![];
    let mut symbols: Vec<(Rc<RefCell<Symbol>>, Value)> = vec![];
    let mut local_symbols: Vec<(Rc<RefCell<Symbol>>, Value)> = vec![];
    //test the symbol
    let mut is_ok = true;
    {
        let sym = symbol.borrow();
        match json {
            Value::Object(details) => {
                for (key, value) in details {
                    is_ok = is_ok && match key.as_str() {
                        "name" => {
                            sym.name == value.as_str().unwrap()
                        },
                        "type" => {
                            sym.sym_type == match value.as_str().unwrap() {
                                "ROOT" => SymType::ROOT,
                                "NAMESPACE" => SymType::NAMESPACE,
                                "PACKAGE" => SymType::PACKAGE,
                                "FILE" => SymType::FILE,
                                "COMPILED" => SymType::COMPILED,
                                "CLASS" => SymType::CLASS,
                                "FUNCTION" => SymType::FUNCTION,
                                "VARIABLE" => SymType::VARIABLE,
                                "CONSTANT" => SymType::CONSTANT,
                                _ => {
                                    error!("Invalid sym_type in json file: {}", value.as_str().unwrap());
                                    SymType::ROOT
                                }
                            }
                        },
                        "module_symbols" => {
                            let mut res = true;
                            for val_mod_sym in value.as_array().expect("module_symbols key should hold an array").iter() {
                                let val_mod_sym_data = val_mod_sym.as_object().expect("module_symbols array should hold objects");
                                let val_mod_sym_name = val_mod_sym_data.get("name").expect("module_symbols object should have a name key").as_str().expect("name key should be a string");
                                let mod_sym = sym.module_symbols.get(val_mod_sym_name);
                                if mod_sym.is_none() {
                                    error!("Module symbol not found in tree: {}", val_mod_sym_name);
                                    res = false;
                                } else {
                                    module_symbols.push((mod_sym.unwrap().clone(), val_mod_sym.clone()));
                                }
                            }
                            for mod_sym in sym.module_symbols.keys() {
                                if value.as_array().unwrap().iter().filter(|x| {x.as_object().unwrap().get("name").unwrap() == mod_sym}).next().is_none() {
                                    error!("Module symbol not found in json: {}", mod_sym);
                                    res = false;
                                }
                            }
                            res
                        },
                        "symbols" => {
                            let mut res = true;
                            for val_mod_sym in value.as_array().expect("symbols key should hold an array").iter() {
                                let val_mod_sym_data = val_mod_sym.as_object().expect("symbols array should hold objects");
                                let val_mod_sym_name = val_mod_sym_data.get("name").expect("symbols object should have a name key").as_str().expect("name key should be a string");
                                let sym = sym.symbols.get(val_mod_sym_name);
                                if sym.is_none() {
                                    error!("Symbol not found in tree: {}", val_mod_sym_name);
                                    res = false;
                                } else {
                                    symbols.push((sym.unwrap().clone(), val_mod_sym.clone()));
                                }
                            }
                            for symbol in sym.symbols.keys() {
                                if value.as_array().unwrap().iter().filter(|x| {x.as_object().unwrap().get("name").unwrap() == symbol}).next().is_none() {
                                    error!("Symbol not found in json: {}", symbol);
                                    res = false;
                                }
                            }
                            res
                        },
                        "local_symbols" => {
                            let mut res = true;
                            if sym.local_symbols.len() != value.as_array().expect("local_symbols key should hold an array").iter().count() {
                                error!("Tree do not contains the same amount of local symbols than json");
                                res = false;
                            }
                            for (json_index, val_mod_sym) in value.as_array().expect("local_symbols key should hold an array").iter().enumerate() {
                                let val_mod_sym_data = val_mod_sym.as_object().expect("local_symbols array should hold objects");
                                let val_mod_sym_name = val_mod_sym_data.get("name").expect("local_symbols object should have a name key").as_str().expect("name key should be a string");
                                let mut loc_sym = None;
                                let mut index = 0;
                                for s in sym.local_symbols.iter() {
                                    if s.borrow().name == val_mod_sym_name {
                                        if index == json_index {
                                            loc_sym = Some(s.clone());
                                            break;
                                        } else {
                                            index += 1;
                                        }
                                    }
                                }
                                if loc_sym.is_none() {
                                    error!("Local symbol not found in json: {}", val_mod_sym_name);
                                    res = false;
                                } else {
                                    local_symbols.push((loc_sym.unwrap().clone(), val_mod_sym.clone()));
                                }
                            }
                            res
                        },
                        "value" => {
                            true
                        }
                        "index" => {
                            true //used at top level
                        }
                        default => {
                            error!("Invalid json format - key {} unknown", default);
                            false
                        }
                    }
                }
            },
            _ => {
                error!("Invalid json format: it should be an object");
            }
        }
    }
    //test subsymbols
    for (sym, val) in module_symbols {
        is_ok = is_ok && _test_symbol_with_json_value(sym, val);
    }
    for (sym, val) in symbols {
        is_ok = is_ok && _test_symbol_with_json_value(sym, val);
    }
    for (sym, val) in local_symbols {
        is_ok = is_ok && _test_symbol_with_json_value(sym, val);
    }
    //return result
    is_ok
}
*/
