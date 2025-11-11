

use std::collections::HashSet;
use std::env;
use odoo_ls_server::{core::evaluation::EvaluationValue, oyarn};
use odoo_ls_server::constants::OYarn;
use odoo_ls_server::utils::PathSanitizer;
use ruff_python_ast::Expr;

use odoo_ls_server::Sy;

mod setup;

#[test]
fn test_no_main_entry() {
    /* First, let's launch the server. It will setup a SyncOdoo struct, with a SyncChannel, that we can use to get the messages that the client would receive. */
    let odoo = setup::setup::setup_server(false);
    assert!(!odoo.has_main_entry);
    assert!(!odoo.has_odoo_main_entry);
    assert!(odoo.entry_point_mgr.borrow().main_entry_point.is_none());
    assert!(odoo.has_valid_python);
}

#[test]
fn test_custom_entry_point() {
    let mut odoo = setup::setup::setup_server(false);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/assign.py");
    let session = setup::setup::prepare_custom_entry_point(&mut odoo, path.sanitize().as_str());
    assert!(odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
}


#[test]
fn test_assigns() {
    let mut odoo = setup::setup::setup_server(false);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/assign.py").sanitize();
    let session = setup::setup::prepare_custom_entry_point(&mut odoo, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);
    let a = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("a")]), u32::MAX);
    assert!(a.len() == 1);
    assert!(a[0].borrow().name() == "a");
    assert!(a[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 5);

    let b = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("b")]), u32::MAX);
    assert!(b.len() == 1);
    assert!(b[0].borrow().name() == "b");
    assert!(b[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::StringLiteral(_))));
    assert!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_string_literal_expr());
    assert!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_string_literal_expr().unwrap().value.to_str() == "test");

    let c = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("c")]), u32::MAX);
    assert!(c.len() == 1);
    assert!(c[0].borrow().name() == "c");
    assert!(c[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_float());
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_float().unwrap() == &3.14);

    let d = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("d")]), u32::MAX);
    assert!(d.len() == 1);
    assert!(d[0].borrow().name() == "d");
    assert!(d[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::BooleanLiteral(_))));
    assert!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_boolean_literal_expr());
    assert!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_boolean_literal_expr().unwrap().value == true);

    let e = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("e")]), u32::MAX);
    assert!(e.len() == 1);
    assert!(e[0].borrow().name() == "e");
    assert!(e[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::BooleanLiteral(_))));
    assert!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_boolean_literal_expr());
    assert!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_boolean_literal_expr().unwrap().value == false);

    let f = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("f")]), u32::MAX);
    assert!(f.len() == 1);
    assert!(f[0].borrow().name() == "f");
    assert!(f[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(f[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(f[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NoneLiteral(_))));
    assert!(f[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_none_literal_expr());

    let g = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("g")]), u32::MAX);
    assert!(g.len() == 1);
    assert!(g[0].borrow().name() == "g");
    assert!(g[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::LIST(_)));
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list().len() == 3);
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[0].is_number_literal_expr());
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[0].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 1);
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[1].is_number_literal_expr());
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[1].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 2);
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[2].is_number_literal_expr());
    assert!(g[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_list()[2].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 3);

    let h = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("h")]), u32::MAX);
    assert!(h.len() == 1);
    assert!(h[0].borrow().name() == "h");
    assert!(h[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::TUPLE(_)));
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple().len() == 3);
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[0].is_number_literal_expr());
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[0].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 1);
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[1].is_number_literal_expr());
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[1].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 2);
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[2].is_number_literal_expr());
    assert!(h[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_tuple()[2].as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 3);

    let i = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("i")]), u32::MAX);
    assert!(i.len() == 1);
    assert!(i[0].borrow().name() == "i");
    assert!(i[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::DICT(_)));
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict().len() == 2);
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].0.is_string_literal_expr());
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].0.as_string_literal_expr().unwrap().value.to_str() == "a");
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].1.is_number_literal_expr());
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[0].1.as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 1);
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].0.is_string_literal_expr());
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].0.as_string_literal_expr().unwrap().value.to_str() == "b");
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].1.is_number_literal_expr());
    assert!(i[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_dict()[1].1.as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 2);

    let j = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![Sy!("j")]), u32::MAX);
    assert!(j.len() == 1);
    assert!(j[0].borrow().name() == "j");
    assert!(j[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(j[0].borrow().evaluations().as_ref().unwrap()[0].value.is_none());

}

#[test]
fn test_sections() {
    let mut odoo = setup::setup::setup_server(false);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/sections.py").sanitize();
    let session = setup::setup::prepare_custom_entry_point(&mut odoo, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);

    let assert_get_int_eval_values = |var_name: &str, values: HashSet<i32>|{
        let syms = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![oyarn!("{}", var_name)]), u32::MAX);
        assert_eq!(syms.len(), values.len()); // Check Number of symbols
        assert_eq!(syms.iter()
        .map(|sym| {
            let sym = sym.borrow();
            assert_eq!(sym.name(), var_name); // Check variable name
            let evaluations = sym.evaluations();
            let eval = evaluations.as_ref().unwrap();
            assert_eq!(eval.len(), 1);  // Check that each symbol has one evaluation
            let value = eval[0].value.as_ref().unwrap();
            assert!(matches!(value, EvaluationValue::CONSTANT(Expr::NumberLiteral(_)))); // Check that the evaluation is a num literal
            let number = value.as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap();
            number.as_i32().unwrap()
        })
        .collect::<HashSet<_>>(), values); // Check evaluation values
    };
    // If statement sections
    assert_get_int_eval_values("a", HashSet::from([5, 6]));
    assert_get_int_eval_values("b", HashSet::from([7]));
    assert_get_int_eval_values("c", HashSet::from([5, 6]));
    assert_get_int_eval_values("d", HashSet::from([4, 5]));
    assert_get_int_eval_values("e", HashSet::from([1, 2 ,3]));
    // For statement sections
    assert_get_int_eval_values("f", HashSet::from([32, 33, 34, 35]));
    assert_get_int_eval_values("g", HashSet::from([98, 99]));
    assert_get_int_eval_values("h", HashSet::from([98, 5]));
    // While statement sections
    assert_get_int_eval_values("i", HashSet::from([67, 76]));
    assert_get_int_eval_values("j", HashSet::from([37, 27]));
    // Try statement sections
    assert_get_int_eval_values("k", HashSet::from([2, 3]));
    assert_get_int_eval_values("l", HashSet::from([30, 40]));
    assert_get_int_eval_values("m", HashSet::from([80]));
    assert_get_int_eval_values("o", HashSet::from([120]));
    assert_get_int_eval_values("p", HashSet::from([20, 30, 40]));
    // Match statement sections
    assert_get_int_eval_values("q", HashSet::from([33, 34, 43]));
    assert_get_int_eval_values("r", HashSet::from([34, 43]));
    // Named expression
    assert_get_int_eval_values("s", HashSet::from([2]));
    assert_get_int_eval_values("t", HashSet::from([3]));
    // If stmt with walrus
    assert_get_int_eval_values("u", HashSet::from([91, 92]));
    assert_get_int_eval_values("v", HashSet::from([72, 73, 74]));
    assert_get_int_eval_values("w", HashSet::from([71, 72, 74]));

}