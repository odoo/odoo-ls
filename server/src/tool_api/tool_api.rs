use std::cell::RefCell;
use std::net::TcpListener;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::io::{prelude::*, ErrorKind};
use std::net::TcpStream;
use std::time::Duration;

use byteyarn::Yarn;
use crossbeam_channel::Select;
use lsp_server::{Connection, Message, Request, RequestId, Response};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use serde_json::json;

use crate::constants::{PackageType, SymType, Tree};
use crate::core::entry_point::{EntryPoint, EntryPointType};
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol::Symbol;
use crate::utils::{tree_yarn_to_string, PathSanitizer};
use crate::Sy;

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
                response_value = ToolAPI::get_symbol(&sync_odoo, serde_json::from_value(request.params).unwrap());
            },
            "$/ToolAPI/get_symbol_with_path" => {
                let sync_odoo = sync_odoo.lock().unwrap();
                response_value = ToolAPI::get_symbol_with_path(&sync_odoo, serde_json::from_value(request.params).unwrap());
            },
            "$/ToolAPI/browse_tree" => {
                let sync_odoo = sync_odoo.lock().unwrap();
                response_value = ToolAPI::browse_tree(&sync_odoo, serde_json::from_value(request.params).unwrap());
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
            not_found.push(json!(tree_yarn_to_string(&symbol.borrow().get_tree())));
        }
        json!({
            "path": entry.path,
            "tree": entry.tree.iter().map(|x| x.to_string()).collect::<Vec<String>>(),
            "type": match entry.typ.clone() {
                EntryPointType::MAIN => "main",
                EntryPointType::ADDON => "addon",
                EntryPointType::BUILTIN => "builtin",
                EntryPointType::PUBLIC => "public",
                EntryPointType::CUSTOM => "custom",
                _ => {"unknown"}
            },
            "addon_to_odoo_path": entry.addon_to_odoo_path,
            "addon_to_odoo_tree": entry.addon_to_odoo_tree.as_ref().map(|x| x.iter().map(|x| x.to_string()).collect::<Vec<String>>()),
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

    fn get_symbol(sync_odoo: &SyncOdoo, params: GetSymbolParams) -> serde_json::Value {
        let mut entry = None;
        let ep_mgr = sync_odoo.entry_point_mgr.borrow();
        for e in ep_mgr.iter_all() {
            if e.borrow().path == params.entry_path {
                entry = Some(e);
                break;
            }
        }
        if let Some(entry) = entry {
            return ToolAPI::symbol_to_json(entry.clone(), &ToolAPI::vec_tree_to_yarn_tree(&params.tree))
        }
        serde_json::Value::Null
    }

    pub fn vec_tree_to_yarn_tree(str_tree: &(Vec<String>, Vec<String>)) -> Tree {
        (str_tree.0.iter().map(|x| Sy!(x.clone())).collect::<Vec<Yarn>>(), str_tree.1.iter().map(|x| Sy!(x.clone())).collect::<Vec<Yarn>>())
    }

    fn symbol_to_json(entry: Rc<RefCell<EntryPoint>>, tree: &Tree) -> serde_json::Value {
        let mut symbols = entry.borrow().root.borrow().get_symbol(tree, u32::MAX);
        if symbols.len() > 1 {
            panic!()
        }
        if tree.0.is_empty() && tree.1.is_empty() {
            symbols.push(entry.borrow().root.clone());
        }
        let Some(symbol) = symbols.first() else {return serde_json::Value::Null};
        let typ = symbol.borrow().typ();
        match typ {
            SymType::ROOT => {
                return symbol.borrow().as_root().to_json();
            }
            SymType::DISK_DIR => {
                return symbol.borrow().as_disk_dir_sym().to_json();
            }
            SymType::NAMESPACE => {
                return symbol.borrow().as_namespace().to_json();
            },
            SymType::PACKAGE(PackageType::MODULE) => {
                return symbol.borrow().as_module_package().to_json();
            },
            SymType::PACKAGE(PackageType::PYTHON_PACKAGE) => {
                return symbol.borrow().as_python_package().to_json();
            },
            SymType::FILE => {
                return symbol.borrow().as_file().to_json();
            },
            SymType::COMPILED => {return symbol.borrow().as_compiled().to_json();},
            SymType::VARIABLE => {
                return symbol.borrow().as_variable().to_json();
            },
            SymType::CLASS => {
                return symbol.borrow().as_class_sym().to_json();
            },
            SymType::FUNCTION => {
                return symbol.borrow().as_func().to_json();
            },
        }
    }

    fn get_symbol_with_path(sync_odoo: &SyncOdoo, params: GetSymbolWithPathParams) -> serde_json::Value {
        let mut entry = None;
        let ep_mgr = sync_odoo.entry_point_mgr.borrow();
        for e in ep_mgr.iter_all() {
            if e.borrow().path == params.entry_path {
                entry = Some(e);
                break;
            }
        }
        let path = PathBuf::from(params.path).to_tree();
        if let Some(entry) = entry {
            return ToolAPI::symbol_to_json(entry.clone(), &path)
        }
        serde_json::Value::Null
    }

    fn browse_tree(sync_odoo: &SyncOdoo, params: BrowseTreeParams) -> serde_json::Value {
        let mut entry = None;
        let ep_mgr = sync_odoo.entry_point_mgr.borrow();
        for e in ep_mgr.iter_all() {
            if e.borrow().path == params.entry_path {
                entry = Some(e);
                break;
            }
        }
        if let Some(entry) = entry {
            let mut symbols = entry.borrow().root.borrow().get_symbol(&ToolAPI::vec_tree_to_yarn_tree(&params.tree), u32::MAX);
            if symbols.len() > 1 {
                panic!()
            }
            if params.tree.0.is_empty() && params.tree.1.is_empty() {
                symbols.push(entry.borrow().root.clone());
            }
            if let Some(symbol) = symbols.first() {
                let has_modules = symbol.borrow().has_modules();
                let module_sym: Vec<Rc<RefCell<Symbol>>> = if has_modules {
                    symbol.borrow().all_module_symbol().map(|x| x.clone()).collect()
                } else {
                    vec![]
                };
                let module_sym: Vec<serde_json::Value> = module_sym.iter().map(|sym| {
                    json!({
                        "name": sym.borrow().name().to_string(),
                        "type": sym.borrow().typ().to_string(),
                    })
                }).collect();
                return json!({
                    "modules": module_sym,
                });
            }
        }
        serde_json::Value::Null
    }
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct BrowseTreeParams {
    pub entry_path: String,
    pub tree: (Vec<String>, Vec<String>),
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct GetSymbolParams {
    pub entry_path: String,
    pub tree: (Vec<String>, Vec<String>),
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct GetSymbolWithPathParams {
    pub entry_path: String,
    pub path: String,
}
