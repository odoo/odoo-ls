

use std::env;
use std::{path::PathBuf, rc::Rc};
use std::cell::RefCell;
use std::fs::File;
use std::io::BufReader;
use odoo_ls_server::core::evaluation::EvaluationValue;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::utils::PathSanitizer;
use ruff_python_ast::Expr;
use serde_json::Value;

use odoo_ls_server::{constants::SymType, core::{entry_point::EntryPointMgr, odoo::SyncOdoo, symbols::symbol::Symbol}, S};
use tracing::error;

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
    let a = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("a")]), u32::MAX);
    assert!(a.len() == 1);
    assert!(a[0].borrow().name() == "a");
    assert!(a[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 5);

    let b = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("b")]), u32::MAX);
    assert!(b.len() == 1);
    assert!(b[0].borrow().name() == "b");
    assert!(b[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::StringLiteral(_))));
    assert!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_string_literal_expr());
    assert!(b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_string_literal_expr().unwrap().value.to_str() == "test");

    let c = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("c")]), u32::MAX);
    assert!(c.len() == 1);
    assert!(c[0].borrow().name() == "c");
    assert!(c[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_float());
    assert!(c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_float().unwrap() == &3.14);

    let d = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("d")]), u32::MAX);
    assert!(d.len() == 1);
    assert!(d[0].borrow().name() == "d");
    assert!(d[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::BooleanLiteral(_))));
    assert!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_boolean_literal_expr());
    assert!(d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_boolean_literal_expr().unwrap().value == true);

    let e = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("e")]), u32::MAX);
    assert!(e.len() == 1);
    assert!(e[0].borrow().name() == "e");
    assert!(e[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::BooleanLiteral(_))));
    assert!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_boolean_literal_expr());
    assert!(e[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_boolean_literal_expr().unwrap().value == false);

    let f = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("f")]), u32::MAX);
    assert!(f.len() == 1);
    assert!(f[0].borrow().name() == "f");
    assert!(f[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(f[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(f[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NoneLiteral(_))));
    assert!(f[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_none_literal_expr());

    let g = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("g")]), u32::MAX);
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

    let h = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("h")]), u32::MAX);
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

    let i = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("i")]), u32::MAX);
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

    let j = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("j")]), u32::MAX);
    assert!(j.len() == 1);
    assert!(j[0].borrow().name() == "j");
    assert!(j[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(j[0].borrow().evaluations().as_ref().unwrap()[0].value.is_none());

}

#[test]
fn test_if_section_assign() {
    let mut odoo = setup::setup::setup_server(false);
    let path = env::current_dir().unwrap().join("tests/data/python/expressions/ifs.py").sanitize();
    let session = setup::setup::prepare_custom_entry_point(&mut odoo, path.as_str());
    assert!(session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.len() == 1);

    let sym_a = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("a")]), u32::MAX);
    assert!(sym_a.len() == 2);
    assert!(sym_a[0].borrow().name() == "a");
    assert!(sym_a[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_a[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_a[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 5);
    assert!(sym_a[1].borrow().name() == "a");
    assert!(sym_a[1].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_a[1].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_a[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_a[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_a[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_a[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 6);

    let sym_b = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("b")]), u32::MAX);
    assert!(sym_b.len() == 1);
    assert!(sym_b[0].borrow().name() == "b");
    assert!(sym_b[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_b[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_b[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 7);


    let sym_c = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("c")]), u32::MAX);
    assert!(sym_c.len() == 2);
    assert!(sym_c[0].borrow().name() == "c");
    assert!(sym_c[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_c[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_c[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 5);
    assert!(sym_c[1].borrow().name() == "c");
    assert!(sym_c[1].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_c[1].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_c[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_c[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_c[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_c[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 6);

    let sym_d = session.sync_odoo.get_symbol(path.as_str(), &(vec![], vec![S!("d")]), u32::MAX);
    assert!(sym_d.len() == 2);
    assert!(sym_d[0].borrow().name() == "d");
    assert!(sym_d[0].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_d[0].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_d[0].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 4);
    assert!(sym_d[1].borrow().name() == "d");
    assert!(sym_d[1].borrow().evaluations().as_ref().unwrap().len() == 1);
    assert!(sym_d[1].borrow().evaluations().as_ref().unwrap()[0].value.is_some());
    assert!(matches!(sym_d[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap(), EvaluationValue::CONSTANT(Expr::NumberLiteral(_))));
    assert!(sym_d[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().is_number_literal_expr());
    assert!(sym_d[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.is_int());
    assert!(sym_d[1].borrow().evaluations().as_ref().unwrap()[0].value.as_ref().unwrap().as_constant().as_number_literal_expr().unwrap().value.as_int().unwrap().as_i32().unwrap() == 5);
}