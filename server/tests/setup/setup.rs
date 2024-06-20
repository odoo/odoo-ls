use std::env;
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tower_lsp::{LspService, Server};
use server::{backend::Backend, core::{config::{Config, DiagMissingImportsMode}, messages::SyncChannel, odoo::SyncOdoo}};
use server::core::messages::MsgHandler;
use server::S;

pub fn setup_server() -> SyncOdoo {
    let community_path = env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder");
    println!("Community path: {:?}", community_path);
    let sync_channel = SyncChannel { messages: RefCell::new(Vec::new()) };
    let msg_handler = MsgHandler::SYNC_CHANNEL(sync_channel);
    let mut server = SyncOdoo::new(msg_handler);
    server.load_odoo_addons = false;

    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path = test_addons_path.join("tests").join("data").join("addons");
    println!("Test addons path: {:?}", test_addons_path);

    let mut config = Config::new();
    config.addons = vec![test_addons_path.to_str().unwrap().to_string()];
    config.odoo_path = community_path;
    config.python_path = S!("python3");
    config.refresh_mode = server::core::config::RefreshMode::Off;
    config.diag_missing_imports = DiagMissingImportsMode::All;
    config.no_typeshed = false;
    server.init(config);

    server
}