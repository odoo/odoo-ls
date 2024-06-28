use std::env;
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use server::{core::{config::{Config, DiagMissingImportsMode}, messages::SyncChannel, odoo::SyncOdoo}, threads::SessionInfo, utils::PathSanitizer as _};
use server::core::messages::MsgHandler;
use server::S;

pub fn setup_server() -> SyncOdoo {
    let community_path = env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder");
    println!("Community path: {:?}", community_path);
    let mut server = SyncOdoo::new();
    server.load_odoo_addons = false;

    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path = test_addons_path.join("tests").join("data").join("addons");
    println!("Test addons path: {:?}", test_addons_path);

    let mut config = Config::new();
    config.addons = vec![test_addons_path.sanitize()];
    config.odoo_path = community_path;
    config.python_path = S!("python3");
    config.refresh_mode = server::core::config::RefreshMode::Off;
    config.diag_missing_imports = DiagMissingImportsMode::All;
    config.no_typeshed = false;

    let (s, r) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s, r, &mut server);
    SyncOdoo::init(&mut session, config);

    server
}