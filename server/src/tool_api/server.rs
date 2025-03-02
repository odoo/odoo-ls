use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use tracing::{error, info};

use crate::core::odoo::SyncOdoo;
use crate::server::Server;
use crate::tool_api::tool_api::ToolAPI;

impl Server {

    pub fn create_spy_connection(&mut self, sync_odoo: Arc<Mutex<SyncOdoo>>) {
        info!("ToolAPI: Creating spy connection");
        let listener = TcpListener::bind("127.0.0.1:8072");
        let Ok(listener) = listener else {
            error!("ToolAPI: Unable to bind to 127.0.0.1:8072. Spy connection will be not available - {}", listener.unwrap_err());
            return;
        };
        self.spy_thread = Some(std::thread::spawn(move || {
            ToolAPI::listen_to_spy(listener, sync_odoo);
        }))
    }
}