use std::env;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::{LspService, Server};
use server::{backend::Backend, core::{messages::SyncChannel, odoo::SyncOdoo}};
use server::core::messages::MsgHandler;

pub fn setup_server() -> SyncOdoo {
    let community_path = env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder");
    let sync_channel = SyncChannel { messages: RefCell::new(Vec::new()) };
    let msg_handler = MsgHandler::SYNC_CHANNEL(sync_channel);
    let server = SyncOdoo::new(msg_handler);

    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path.push("resources/test");

    server
}