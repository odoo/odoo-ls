use crate::setup::shared::with_shared_session;
use std::env;
use std::path::Path;

use odoo_ls_server::utils::PathSanitizer;


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
    with_shared_session(true, |session| {

        let odoo_path = Path::new(&env::var("COMMUNITY_PATH").unwrap()).sanitize();

        /* Let's ensure that the architecture is loaded */
        assert!(!session.sync_odoo.get_symbol(odoo_path.as_str(), (&["odoo"], &[]), u32::MAX).is_empty());
        /* Let's ensure that odoo/addons is loaded */
        assert!(!session.sync_odoo.get_symbol(odoo_path.as_str(), (&["odoo", "addons"], &[]), u32::MAX).is_empty());
        /* And let's test that our test module has well been added and available in odoo/addons */
        assert!(!session.sync_odoo.get_symbol(odoo_path.as_str(), (&["odoo", "addons", "module_1"], &[]), u32::MAX).is_empty());
    });
}
