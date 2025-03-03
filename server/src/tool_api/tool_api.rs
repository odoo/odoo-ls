use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::io::{prelude::*, ErrorKind};
use std::net::TcpStream;
use std::time::Duration;

use crossbeam_channel::Select;
use lsp_server::{Connection, Message, Request, RequestId, Response};
use tracing::{error, info};
use serde_json::json;

use crate::core::entry_point::{EntryPoint, EntryPointType};
use crate::core::odoo::SyncOdoo;

use super::io_threads::ToolAPIIoThreads;
use super::socket;

pub static CAN_TOOL_API_RUN: AtomicBool = AtomicBool::new(true);

pub struct ToolAPI {

}

impl ToolAPI {
    pub fn listen_to_spy(listener: TcpListener, sync_odoo: Arc<Mutex<SyncOdoo>>) {
        let mut threads = vec![];
        while CAN_TOOL_API_RUN.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let (sender, receiver, io_threads) = socket::socket_transport(stream);
                    let sync_odoo = sync_odoo.clone();
                    threads.push(std::thread::spawn(move || Self::handle_connection(Connection { sender, receiver }, io_threads, sync_odoo)))
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // No connection yet, so just sleep for a bit to prevent busy waiting
                    thread::sleep(Duration::from_millis(2000));
                }
                Err(e) => {
                    eprintln!("Connection failed: {}", e);
                    break;
                }
            }
        }
        for thread in threads {
            thread.join().unwrap();
        }
    }

    fn handle_connection(connection: Connection, io_threads: ToolAPIIoThreads, sync_odoo: Arc<Mutex<SyncOdoo>>) {
        info!("ToolAPI: Connection established");
        while CAN_TOOL_API_RUN.load(Ordering::SeqCst) {
            let msg = connection.receiver.try_recv();
            if let Err(ref e) = msg {
                match e {
                    crossbeam_channel::TryRecvError::Empty => {
                        thread::sleep(Duration::from_millis(80));
                        continue;
                    },
                    crossbeam_channel::TryRecvError::Disconnected => {
                        info!("ToolAPI: Connection closed");
                        break;
                    }
                }
            }
            ToolAPI::handle_msg(&connection, msg.unwrap(), &sync_odoo);
        }
        io_threads.join();
    }

    fn handle_msg(connection: &Connection, msg: Message, sync_odoo: &Arc<Mutex<SyncOdoo>>) {
        match msg {
            Message::Request(req) => {
                let response = ToolAPI::handle_request(&sync_odoo, req);
                if let Err(e) = connection.sender.send(Message::Response(response)) {
                    error!("ToolAPI: Error sending response: {}", e);
                }
            },
            Message::Notification(not) => {
                ToolAPI::handle_notification(&sync_odoo, not);
            },
            Message::Response(_) => {
                error!("ToolAPI: Unexpected response");
            }
        }
    }

    fn handle_request(sync_odoo: &Arc<Mutex<SyncOdoo>>, request: Request) -> Response {
        let response_value;
        match request.method.as_str() {
            "$/ToolAPI/list_entries" => {
                let sync_odoo = sync_odoo.lock().unwrap();
                response_value = ToolAPI::list_entries(&sync_odoo);
            },
            "$/ToolAPI/get_symbol" => {
                let sync_odoo = sync_odoo.lock().unwrap();
                response_value = ToolAPI::get_symbol(&sync_odoo, &request.params);
            },
            _ => {
                error!("ToolAPI: Unknown request method: {}", request.method);
                return Response::new_err(request.id.clone(), lsp_server::ErrorCode::MethodNotFound as i32, format!("Method not found {}", request.method));
            }
        }
        Response::new_ok(request.id.clone(), response_value)
    }

    fn handle_notification(sync_odoo: &Arc<Mutex<SyncOdoo>>, _notification: lsp_server::Notification) {
        let sync_odoo = sync_odoo.lock().unwrap();
        // Do nothing
    }

    fn entry_to_json(entry: &EntryPoint) -> serde_json::Value {
        let mut not_found = vec![];
        for symbol in entry.not_found_symbols.iter() {
            not_found.push(json!(symbol.borrow().get_tree()));
        }
        json!({
            "path": entry.path,
            "tree": entry.tree,
            "type": match entry.typ.clone() {
                EntryPointType::MAIN => "main",
                EntryPointType::ADDON => "addon",
                EntryPointType::BUILTIN => "builtin",
                EntryPointType::PUBLIC => "public",
                EntryPointType::CUSTOM => "custom",
                _ => {"unknown"}
            },
            "addon_to_odoo_path": entry.addon_to_odoo_path,
            "addon_to_odoo_tree": entry.addon_to_odoo_tree,
            "not_found_symbols": not_found,
        })
    }

    fn list_entries(sync_odoo: &SyncOdoo) -> serde_json::Value {
        let mut entries = vec![];
        for entry in sync_odoo.entry_point_mgr.borrow().iter_all() {
            entries.push(ToolAPI::entry_to_json(&entry.borrow()));
        }
        serde_json::Value::Array(entries)
    }

    fn get_symbol(sync_odoo: &SyncOdoo, params: &serde_json::Value) -> serde_json::Value {
        /*let path = params["path"].as_str().unwrap();
        let tree = params["tree"].as_str().unwrap();
        let entry = sync_odoo.entry_point_mgr.borrow().get_entry(path, tree);
        match entry {
            Some(entry) => ToolAPI::entry_to_json(&entry.borrow()),
            None => serde_json::Value::Null
        }*/
        serde_json::Value::Null
    }
}