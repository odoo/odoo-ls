use std::env;

use std::path::PathBuf;


use server::{core::{config::{Config, DiagMissingImportsMode}, odoo::SyncOdoo}, threads::SessionInfo, utils::PathSanitizer as _};

use server::S;
use tracing::info;

pub fn setup_server() -> SyncOdoo {
    let community_path = env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder");
    info!("Community path: {:?}", community_path);
    let mut server = SyncOdoo::new();
    server.load_odoo_addons = false;

    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path = test_addons_path.join("tests").join("data").join("addons");
    info!("Test addons path: {:?}", test_addons_path);

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