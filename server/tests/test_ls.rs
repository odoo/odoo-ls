

use std::rc::Rc;
use std::cell::RefCell;
use std::fs::File;
use std::io::BufReader;
use serde_json::Value;

use odoo_ls_server::{S, core::symbol::Symbol, constants::SymType};
use tracing::error;

mod setup;

#[test]
fn test_structure() {
    /* First, let's launch the server. It will setup a SyncOdoo struct, with a SyncChannel, that we can use to get the messages that the client would receive. */
    let odoo = setup::setup::setup_server();

    assert!(odoo.get_symbol(&(vec![S!("odoo")], vec![])).is_some());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons")], vec![])).is_some());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1")], vec![])).is_some());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_2")], vec![])).is_some());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("not_a_module")], vec![])).is_none());

    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1"), S!("not_loaded")], vec![])).is_none());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1"), S!("not_loaded"), S!("not_loaded_file")], vec![])).is_none());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1"), S!("not_loaded"), S!("not_loaded_file")], vec![S!("NotLoadedClass")])).is_none());
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1"), S!("not_loaded"), S!("not_loaded_file")], vec![S!("NotLoadedFunc")])).is_none());

    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1"), S!("models")], vec![])).is_some());
    let models = odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1"), S!("models")], vec![])).unwrap();
    assert!(models.borrow().get_symbol(&(vec![S!("base_test_models")], vec![])).is_some());
    assert!(models.borrow().get_symbol(&(vec![], vec![S!("base_test_models")])).is_some());
    assert!(!Rc::ptr_eq(&models.borrow().get_symbol(&(vec![S!("base_test_models")], vec![])).unwrap(),
            &models.borrow().get_symbol(&(vec![], vec![S!("base_test_models")])).unwrap()));
    assert!(Rc::ptr_eq(&models.borrow().symbols["base_test_models"], &models.borrow().get_symbol(&(vec![], vec![S!("base_test_models")])).unwrap()));
    assert!(Rc::ptr_eq(&models.borrow().module_symbols["base_test_models"], &models.borrow().get_symbol(&(vec![S!("base_test_models")], vec![])).unwrap()));
    let module_1 = odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1")], vec![])).unwrap();
    assert!(compare_symbol_with_json(module_1, "tests/module_1_structure.json"))
}

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
                                "DIRTY" => SymType::DIRTY,
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
                                    error!("Module symbol not found: {}", val_mod_sym_name);
                                    res = false;
                                } else {
                                    module_symbols.push((mod_sym.unwrap().clone(), val_mod_sym.clone()));
                                }
                            }
                            res
                        },
                        "symbols" => {
                            let mut res = true;
                            for val_mod_sym in value.as_array().expect("module_symbols key should hold an array").iter() {
                                let val_mod_sym_data = val_mod_sym.as_object().expect("module_symbols array should hold objects");
                                let val_mod_sym_name = val_mod_sym_data.get("name").expect("module_symbols object should have a name key").as_str().expect("name key should be a string");
                                let sym = sym.symbols.get(val_mod_sym_name);
                                if sym.is_none() {
                                    error!("Symbol not found: {}", val_mod_sym_name);
                                    res = false;
                                } else {
                                    symbols.push((sym.unwrap().clone(), val_mod_sym.clone()));
                                }
                            }
                            res
                        },
                        "local_symbols" => {
                            let mut res = true;
                            for val_mod_sym in value.as_array().expect("module_symbols key should hold an array").iter() {
                                let val_mod_sym_data = val_mod_sym.as_object().expect("module_symbols array should hold objects");
                                let val_mod_sym_name = val_mod_sym_data.get("name").expect("module_symbols object should have a name key").as_str().expect("name key should be a string");
                                let mut loc_sym = None;
                                let mut index = 0;
                                for s in sym.local_symbols.iter() {
                                    if s.borrow().name == val_mod_sym_name {
                                        if index == val_mod_sym_data.get("index").expect("local_symbols object should have an index key").as_u64().expect("index key should be an integer") {
                                            loc_sym = Some(s.clone());
                                            break;
                                        } else {
                                            index += 1;
                                        }
                                    }
                                }
                                if loc_sym.is_none() {
                                    error!("Local symbol not found: {}", val_mod_sym_name);
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