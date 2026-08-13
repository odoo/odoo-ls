

use odoo_ls_server::utils::HashSet;
use std::env;
use odoo_ls_server::{core::evaluation::EvaluationValue};
use odoo_ls_server::utils::PathSanitizer;
use ruff_python_ast::Expr;


mod setup;

#[test]
fn test_no_main_entry() {
    /* First, let's launch the server. It will setup a SyncOdoo struct, with a SyncChannel, that we can use to get the messages that the client would receive. */
    let (mut odoo, config) = setup::setup::setup_server(false);
    let _ = setup::setup::create_init_session(&mut odoo, config);
    assert!(!odoo.has_main_entry);
    assert!(!odoo.has_odoo_main_entry);
    assert!(odoo.entry_point_mgr.borrow().main_entry_point.is_none());
    assert!(odoo.has_valid_python);
}

#[test]
fn test_custom_entry_point() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/assign.py");
    setup::setup::prepare_custom_entry_point(&mut session, path.sanitize().as_str());
    assert!(odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
}


#[test]
fn test_assigns() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/assign.py").sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
    let st = &session.sync_odoo.symbol_table;
    let int_type = session.sync_odoo.get_symbol("", (&["builtins"], &["int"]), u32::MAX)[0];
    let str_type = session.sync_odoo.get_symbol("", (&["builtins"], &["str"]), u32::MAX)[0];

    let a = session.sync_odoo.get_symbol(path.as_str(), (&[], &["a"]), u32::MAX);
    assert!(a.len() == 1);
    assert!(st.name(a[0]) == "a");
    assert!(st.evaluations(a[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(a[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(a[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::NumberLiteral(_))));
    assert!(st.evaluations(a[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(st.evaluations(a[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(st.evaluations(a[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 5);

    let b = session.sync_odoo.get_symbol(path.as_str(), (&[], &["b"]), u32::MAX);
    assert!(b.len() == 1);
    assert!(st.name(b[0]) == "b");
    assert!(st.evaluations(b[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(b[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(b[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::StringLiteral(_))));
    assert!(st.evaluations(b[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_string_literal_expr());
    assert!(st.evaluations(b[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_string_literal_expr().unwrap().value.to_str() == "test");

    let c = session.sync_odoo.get_symbol(path.as_str(), (&[], &["c"]), u32::MAX);
    assert!(c.len() == 1);
    assert!(st.name(c[0]) == "c");
    assert!(st.evaluations(c[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(c[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(c[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::NumberLiteral(_))));
    assert!(st.evaluations(c[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(st.evaluations(c[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_float());
    assert!(st.evaluations(c[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_float().unwrap() == &6.66);

    let d = session.sync_odoo.get_symbol(path.as_str(), (&[], &["d"]), u32::MAX);
    assert!(d.len() == 1);
    assert!(st.name(d[0]) == "d");
    assert!(st.evaluations(d[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(d[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(d[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::BooleanLiteral(_))));
    assert!(st.evaluations(d[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_boolean_literal_expr());
    assert!(st.evaluations(d[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_boolean_literal_expr().unwrap().value);

    let e = session.sync_odoo.get_symbol(path.as_str(), (&[], &["e"]), u32::MAX);
    assert!(e.len() == 1);
    assert!(st.name(e[0]) == "e");
    assert!(st.evaluations(e[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(e[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(e[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::BooleanLiteral(_))));
    assert!(st.evaluations(e[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_boolean_literal_expr());
    assert!(!st.evaluations(e[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_boolean_literal_expr().unwrap().value);

    let f = session.sync_odoo.get_symbol(path.as_str(), (&[], &["f"]), u32::MAX);
    assert!(f.len() == 1);
    assert!(st.name(f[0]) == "f");
    assert!(st.evaluations(f[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(f[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(f[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::NoneLiteral(_))));
    assert!(st.evaluations(f[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_none_literal_expr());

    let g = session.sync_odoo.get_symbol(path.as_str(), (&[], &["g"]), u32::MAX);
    assert!(g.len() == 1);
    assert!(st.name(g[0]) == "g");
    assert!(st.evaluations(g[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::LIST(_)));
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list().len() == 3);
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[0].is_number_literal_expr());
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[0].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 1);
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[1].is_number_literal_expr());
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[1].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 2);
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[2].is_number_literal_expr());
    assert!(st.evaluations(g[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[2].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 3);

    let h = session.sync_odoo.get_symbol(path.as_str(), (&[], &["h"]), u32::MAX);
    assert!(h.len() == 1);
    assert!(st.name(h[0]) == "h");
    assert!(st.evaluations(h[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::TUPLE(_)));
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple().len() == 3);
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[0].is_number_literal_expr());
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[0].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 1);
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[1].is_number_literal_expr());
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[1].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 2);
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[2].is_number_literal_expr());
    assert!(st.evaluations(h[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[2].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 3);

    let i = session.sync_odoo.get_symbol(path.as_str(), (&[], &["i"]), u32::MAX);
    assert!(i.len() == 1);
    assert!(st.name(i[0]) == "i");
    assert!(st.evaluations(i[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.is_some());
    assert!(matches!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::DICT(_)));
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict().len() == 2);
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].0.is_string_literal_expr());
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].0.as_string_literal_expr().unwrap().value.to_str() == "a");
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].1.is_number_literal_expr());
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].1.as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 1);
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].0.is_string_literal_expr());
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].0.as_string_literal_expr().unwrap().value.to_str() == "b");
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].1.is_number_literal_expr());
    assert!(st.evaluations(i[0]).as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].1.as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 2);

    let j = session.sync_odoo.get_symbol(path.as_str(), (&[], &["j"]), u32::MAX);
    assert!(j.len() == 1);
    assert!(st.name(j[0]) == "j");
    assert!(st.evaluations(j[0]).as_ref().unwrap().len() == 1);
    assert!(st.evaluations(j[0]).as_ref().unwrap()[0].value.is_none());

    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x"]), u32::MAX);
    assert!(x.len() == 1);
    assert!(st.name(x[0]) == "x");
    assert!(st.evaluations(x[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y"]), u32::MAX);
    assert!(y.len() == 1);
    assert!(st.name(y[0]) == "y");
    assert!(st.evaluations(y[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(y[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);

    let x2 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x2"]), u32::MAX);
    assert!(x2.len() == 1);
    assert!(st.name(x2[0]) == "x2");
    assert!(st.evaluations(x2[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x2[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y2 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y2"]), u32::MAX);
    assert!(y2.len() == 1);
    assert!(st.name(y2[0]) == "y2");
    assert!(st.evaluations(y2[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(y2[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);

    let x3 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x3"]), u32::MAX);
    assert!(x3.len() == 1);
    assert!(st.name(x3[0]) == "x3");
    assert!(st.evaluations(x3[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x3[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y3 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y3"]), u32::MAX);
    assert!(y3.len() == 1);
    assert!(st.name(y3[0]) == "y3");
    assert!(st.evaluations(y3[0]).as_ref().unwrap().is_empty());
    let z3 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["z3"]), u32::MAX);
    assert!(z3.len() == 1);
    assert!(st.name(z3[0]) == "z3");
    assert!(st.evaluations(z3[0]).as_ref().unwrap().is_empty());

    let x4 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x4"]), u32::MAX);
    assert!(x4.len() == 1);
    assert!(st.name(x4[0]) == "x4");
    assert!(st.evaluations(x4[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x4[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y4 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y4"]), u32::MAX);
    assert!(y4.len() == 1);
    assert!(st.name(y4[0]) == "y4");
    assert!(st.evaluations(y4[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(y4[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let z4 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["z4"]), u32::MAX);
    assert!(z4.len() == 1);
    assert!(st.name(z4[0]) == "z4");
    assert!(st.evaluations(z4[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(z4[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == str_type);

    let x5 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x5"]), u32::MAX);
    assert!(x5.len() == 1);
    assert!(st.name(x5[0]) == "x5");
    assert!(st.evaluations(x5[0]).as_ref().unwrap().is_empty());
    let y5 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y5"]), u32::MAX);
    assert!(y5.len() == 1);
    assert!(st.name(y5[0]) == "y5");
    assert!(st.evaluations(y5[0]).as_ref().unwrap().is_empty());

    let x6 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x6"]), u32::MAX);
    assert!(x6.len() == 1);
    assert!(st.name(x6[0]) == "x6");
    assert!(st.evaluations(x6[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x6[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y6 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y6"]), u32::MAX);
    assert!(y6.len() == 1);
    assert!(st.name(y6[0]) == "y6");
    assert!(st.evaluations(y6[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(y6[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);

    let x7 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x7"]), u32::MAX);
    assert!(x7.len() == 1);
    assert!(st.name(x7[0]) == "x7");
    assert!(st.evaluations(x7[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(x7[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);
    let y7 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y7"]), u32::MAX);
    assert!(y7.len() == 1);
    assert!(st.name(y7[0]) == "y7");
    assert!(st.evaluations(y7[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(y7[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.is_some());
    let eval = eval.unwrap();
    assert!(eval == int_type);

}

#[test]
fn test_ann_assign_invalid_target() {
    // Regression test: annotated assignment with a tuple/list target is invalid Python
    // (only a single Name/Attribute/Subscript target can be annotated - and only a
    // single target can be annotated without a value at all), but ruff's error-recovery
    // parser still produces a StmtAnnAssign for it - a very plausible mid-typing state.
    // This must not panic. Since there's no value and more than one target, unpack_assign
    // now deliberately produces no Assign at all for these (the `if value.is_some()`
    // guard on the Tuple/List fallback arm), so no variable is created for them - same
    // as if the (invalid) statement weren't there. A single annotated target with no
    // value is still valid and must keep working normally.
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/ann_assign_invalid_target.py").sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
    let int_type = session.sync_odoo.get_symbol("", (&["builtins"], &["int"]), u32::MAX)[0];
    let st = &session.sync_odoo.symbol_table;

    let x = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x"]), u32::MAX);
    assert!(x.is_empty());
    let y = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y"]), u32::MAX);
    assert!(y.is_empty());

    let x2 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x2"]), u32::MAX);
    assert!(x2.is_empty());
    let y2 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y2"]), u32::MAX);
    assert!(y2.is_empty());

    // "x3, y3:" - annotation not typed yet either; must not panic regardless.
    let x3 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["x3"]), u32::MAX);
    assert!(x3.is_empty());
    let y3 = session.sync_odoo.get_symbol(path.as_str(), (&[], &["y3"]), u32::MAX);
    assert!(y3.is_empty());

    // A single annotated target with no value is valid Python and must still work.
    let single_target = session.sync_odoo.get_symbol(path.as_str(), (&[], &["single_target"]), u32::MAX);
    assert!(single_target.len() == 1);
    assert!(st.evaluations(single_target[0]).as_ref().unwrap().len() == 1);
    let eval = st.evaluations(single_target[0]).as_ref().unwrap()[0].symbol.get_symbol_ptr().upgrade_weak(&session.sync_odoo.symbol_table);
    assert!(eval.unwrap() == int_type);
}

#[test]
fn test_sections() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/sections.py").sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
    let st = &session.sync_odoo.symbol_table;

    let assert_get_int_eval_values = |var_name: &str, values: HashSet<i32>|{
        let syms = session.sync_odoo.get_symbol(path.as_str(), (&[], &[var_name]), u32::MAX);
        assert_eq!(syms.len(), values.len()); // Check Number of symbols
        assert_eq!(syms.iter()
        .map(|&sym| {
            assert_eq!(st.name(sym), var_name); // Check variable name
            let evaluations = st.evaluations(sym);
            let eval = evaluations.as_ref().unwrap();
            assert_eq!(eval.len(), 1);  // Check that each symbol has one evaluation
            let value = eval[0].value.as_ref().unwrap();
            assert!(matches!(value, EvaluationValue::CONSTANT(c) if matches!(c.as_ref(), Expr::NumberLiteral(_)))); // Check that the evaluation is a num literal
            let number = value.as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap();
            number.as_i32().unwrap()
        })
        .collect::<HashSet<_>>(), values); // Check evaluation values
    };
    // If statement sections
    assert_get_int_eval_values("a", HashSet::from_iter([5, 6]));
    assert_get_int_eval_values("b", HashSet::from_iter([7]));
    assert_get_int_eval_values("c", HashSet::from_iter([5, 6]));
    assert_get_int_eval_values("d", HashSet::from_iter([4, 5]));
    assert_get_int_eval_values("e", HashSet::from_iter([1, 2 ,3]));
    // For statement sections
    assert_get_int_eval_values("f", HashSet::from_iter([32, 33, 34, 35]));
    assert_get_int_eval_values("g", HashSet::from_iter([98, 99]));
    assert_get_int_eval_values("h", HashSet::from_iter([98, 5]));
    // While statement sections
    assert_get_int_eval_values("i", HashSet::from_iter([67, 76]));
    assert_get_int_eval_values("j", HashSet::from_iter([37, 27]));
    // Try statement sections
    assert_get_int_eval_values("k", HashSet::from_iter([2, 3]));
    assert_get_int_eval_values("l", HashSet::from_iter([30, 40]));
    assert_get_int_eval_values("m", HashSet::from_iter([80]));
    assert_get_int_eval_values("o", HashSet::from_iter([120]));
    assert_get_int_eval_values("p", HashSet::from_iter([20, 30, 40]));
    // Match statement sections
    assert_get_int_eval_values("q", HashSet::from_iter([33, 34, 43]));
    assert_get_int_eval_values("r", HashSet::from_iter([34, 43]));
    // Named expression
    assert_get_int_eval_values("s", HashSet::from_iter([2]));
    assert_get_int_eval_values("t", HashSet::from_iter([3]));
    // If stmt with walrus
    assert_get_int_eval_values("u", HashSet::from_iter([91, 92]));
    assert_get_int_eval_values("v", HashSet::from_iter([72, 73, 74]));
    assert_get_int_eval_values("w", HashSet::from_iter([71, 72, 74]));

}

#[test]
fn test_star_import_on_disk_dir_does_not_panic() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = env::current_dir().unwrap().join("tests/data/python/disk_dir_import/star_import.py").sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
}
