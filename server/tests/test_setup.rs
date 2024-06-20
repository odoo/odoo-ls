use std::env;
use std::path::Path;

use server::S;

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
    let odoo = setup::setup::setup_server();

    /* Let's ensure that the architecture is loaded */
    assert!(odoo.get_symbol(&(vec![S!("odoo")], vec![])).is_some());
    /* Let's ensure that odoo/addons is loaded */
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons")], vec![])).is_some());
    /* And let's test that our test module has well been added and available in odoo/addons */
    assert!(odoo.get_symbol(&(vec![S!("odoo"), S!("addons"), S!("module_1")], vec![])).is_some());
}