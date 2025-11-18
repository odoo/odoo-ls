use std::env;
use std::path::{Path, PathBuf};

use odoo_ls_server::constants::OYarn;
use odoo_ls_server::utils::PathSanitizer;
use odoo_ls_server::Sy;

mod setup;

/* This file tests that your setup is working, and show you how to write tests. */

#[test]
fn test_setup() {
    assert!(env::var("COMMUNITY_PATH").is_ok(), "Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder");
    assert!(Path::new(&env::var("COMMUNITY_PATH").unwrap()).exists());
    assert!(Path::new(&env::var("COMMUNITY_PATH").unwrap()).join("odoo").exists());
    assert!(Path::new(&env::var("COMMUNITY_PATH").unwrap()).join("odoo").join("release.py").exists());
}

#[test]
fn test_start_odoo_server() {
    /* First, let's launch the server. It will setup a SyncOdoo struct, with a SyncChannel, that we can use to get the messages that the client would receive. */
    let (mut odoo, config) = setup::setup::setup_server(true);
    let _ = setup::setup::create_init_session(&mut odoo, config);

    let odoo_path = PathBuf::from(env::var("COMMUNITY_PATH").unwrap()).sanitize();

    /* Let's ensure that the architecture is loaded */
    assert!(!odoo.get_symbol(odoo_path.as_str(), &(vec![Sy!("odoo")], vec![]), u32::MAX).is_empty());
    /* Let's ensure that odoo/addons is loaded */
    assert!(!odoo.get_symbol(odoo_path.as_str(), &(vec![Sy!("odoo"), Sy!("addons")], vec![]), u32::MAX).is_empty());
    /* And let's test that our test module has well been added and available in odoo/addons */
    assert!(!odoo.get_symbol(odoo_path.as_str(), &(vec![Sy!("odoo"), Sy!("addons"), Sy!("module_1")], vec![]), u32::MAX).is_empty());
}