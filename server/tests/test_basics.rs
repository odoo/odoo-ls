

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
}