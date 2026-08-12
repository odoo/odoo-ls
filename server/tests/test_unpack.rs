use std::env;

use odoo_ls_server::utils::PathSanitizer as _;

mod setup;


#[test]
fn test_unpack() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/for_loop_unpack.py").sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.custom_entry_points.len() == 1);
    let st = &session.sync_odoo.symbol_table;

    let int_type = session.sync_odoo.get_symbol("", (&["builtins"], &["int"]), u32::MAX)[0];
    let str_type = session.sync_odoo.get_symbol("", (&["builtins"], &["str"]), u32::MAX)[0];
    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x"]), u32::MAX);
    assert!(x.len() == 1);
    let x = x[0];
    assert!(st.name(x) == "x");
    assert!(st.evaluations(x).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(x).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);


    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x1"]), u32::MAX);
    assert!(x.len() == 1);
    let x = x[0];
    assert!(st.name(x) == "x1");
    assert!(st.evaluations(x).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(x).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let eval_str = st.evaluations(x).as_ref().unwrap()[1].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval_str.is_some());
    let eval_str = eval_str.unwrap();
    assert!(eval_str == str_type);


    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x2"]), u32::MAX);
    assert!(x.len() == 1);
    let x = x[0];
    assert!(st.name(x) == "x2");
    assert!(st.evaluations(x).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(x).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y2"]), u32::MAX);
    assert!(y.len() == 1);
    let y = y[0];
    assert!(st.name(y) == "y2");
    assert!(st.evaluations(y).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(y).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);


    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x3"]), u32::MAX);
    assert!(x.len() == 1);
    let x = x[0];
    assert!(st.name(x) == "x3");
    assert!(st.evaluations(x).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(x).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y3"]), u32::MAX);
    assert!(y.len() == 1);
    let y = y[0];
    assert!(st.name(y) == "y3");
    assert!(st.evaluations(y).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(y).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);

    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x4"]), u32::MAX);
    assert!(x.len() == 1);
    let x = x[0];
    assert!(st.name(x) == "x4");
    assert!(st.evaluations(x).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);

    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x5"]), u32::MAX);
    assert!(x.len() == 1);
    let x = x[0];
    assert!(st.name(x) == "x5");
    assert!(st.evaluations(x).as_ref().unwrap().len() == 2);
    let eval = st.evaluations(x).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let eval = st.evaluations(x).as_ref().unwrap()[1].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == str_type);

}