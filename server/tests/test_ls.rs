

use std::path::PathBuf;
use std::{env, rc::Rc};
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;
use odoo_ls_server::Sy;
use odoo_ls_server::constants::OYarn;

use weak_table::traits::WeakElement;

mod setup;

#[test]
fn test_structure() {
    /* First, let's launch the server. It will setup a SyncOdoo struct, with a SyncChannel, that we can use to get the messages that the client would receive. */
    let (mut odoo, config) = setup::setup::setup_server(true);
    let _ = setup::setup::create_init_session(&mut odoo, config);

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = PathBuf::from(odoo_path).sanitize();
    let odoo_path = odoo_path.as_str();
    assert!(!odoo.get_symbol(odoo_path, &(vec![Sy!("odoo")], vec![]), u32::MAX).is_empty());
    assert!(!odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons")], vec![]), u32::MAX).is_empty());
    assert!(!odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1")], vec![]), u32::MAX).is_empty());
    assert!(!odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_2")], vec![]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("not_a_module")], vec![]), u32::MAX).is_empty());

    assert!(odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("not_loaded")], vec![]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("not_loaded"), Sy!("not_loaded_file")], vec![]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("not_loaded"), Sy!("not_loaded_file")], vec![Sy!("NotLoadedClass")]), u32::MAX).is_empty());
    assert!(odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("not_loaded"), Sy!("not_loaded_file")], vec![Sy!("NotLoadedFunc")]), u32::MAX).is_empty());

    let models = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("models")], vec![]), u32::MAX);
    assert!(models.len() == 1);
    assert!(models[0].borrow().get_symbol(&(vec![Sy!("base_test_models")], vec![]), u32::MAX).len() == 1);
    assert!(models[0].borrow().get_symbol(&(vec![], vec![Sy!("base_test_models")]), u32::MAX).len() == 1);
    assert!(!Rc::ptr_eq(&models[0].borrow().get_symbol(&(vec![Sy!("base_test_models")], vec![]), u32::MAX)[0],
            &models[0].borrow().get_symbol(&(vec![], vec![Sy!("base_test_models")]), u32::MAX)[0]));
    let module_1 = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1")], vec![]), u32::MAX);
    assert!(module_1.len() == 1);
    //assert!(compare_symbol_with_json(module_1, "tests/module_1_structure.json"))
    test_imports(&odoo);
}

fn test_imports(odoo: &SyncOdoo) {
    //test direct imports
    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = PathBuf::from(odoo_path).sanitize();
    let odoo_path = odoo_path.as_str();
    let model_var = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1")], vec![Sy!("models")]), u32::MAX);
    let model_dir = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("models")], vec![]), u32::MAX);
    assert!(model_var.len() == 1);
    assert!(model_dir.len() == 1);
    assert!(!Rc::ptr_eq(&model_dir[0], &model_var[0]));
    assert!(model_var[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(Rc::ptr_eq(&model_dir[0], &model_var[0].borrow().evaluations().as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak().unwrap()));
    let data_var = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1")], vec![Sy!("data")]), u32::MAX);
    assert!(data_var.len() == 1);
    assert!(data_var[0].borrow().evaluations().as_ref().unwrap().is_empty());

    //test * imports
    let constants_dir = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("constants")], vec![]), u32::MAX);
    assert!(constants_dir.len() == 1);
    let constants_dir = constants_dir[0].clone();
    assert!(constants_dir.borrow().all_symbols().collect::<Vec<_>>().len() == 3);
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_1")]), u32::MAX).len() == 1);
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX).len() == 1);
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_3")]), u32::MAX).len() == 0);
    assert!(constants_dir.borrow().get_symbol(&(vec![Sy!("data")], vec![]), u32::MAX).len() == 1);
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_1")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_1")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap()[0].value.is_none());
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(constants_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap()[0].value.is_none());
    let data_dir = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("constants"), Sy!("data")], vec![]), u32::MAX);
    assert!(data_dir.len() == 1);
    let data_dir = data_dir[0].clone();
    assert!(data_dir.borrow().all_symbols().collect::<Vec<_>>().len() == 4);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_1")]), u32::MAX).len() == 1);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX).len() == 1);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_3")]), u32::MAX).len() == 0);
    assert!(data_dir.borrow().get_symbol(&(vec![Sy!("constants")], vec![]), u32::MAX).len() == 1);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_1")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_1")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap()[0].value.is_none());
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), u32::MAX)[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 22);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), 26)[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), 26)[0].borrow().evaluations().as_ref().unwrap()[0].value.is_none());
    assert!(!data_dir.borrow().get_symbol(&(vec![], vec![Sy!("CONSTANT_2")]), 26)[0].borrow().evaluations().as_ref().unwrap()[0].symbol.get_weak().weak.is_expired());

    //Test odoo.addons import
    let constant_1_var = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("models"), Sy!("base_test_models")], vec![Sy!("CONSTANT_1")]), u32::MAX);
    println!("test");
    assert!(constant_1_var.len() == 1);
    assert!(constant_1_var[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    let constant_1_var_data = odoo.get_symbol(odoo_path, &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1"), Sy!("constants")], vec![Sy!("CONSTANT_1")]), u32::MAX);
    assert!(constant_1_var_data.len() == 1);
    assert!(Rc::ptr_eq(&constant_1_var_data[0], &constant_1_var[0].borrow().evaluations().as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak().unwrap()));

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