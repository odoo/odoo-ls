use std::{collections::HashMap, fmt, io::Error, panic, sync::{atomic::AtomicBool, Arc, Mutex}, thread::JoinHandle};

use clap::error;
use crossbeam_channel::{Receiver, RecvTimeoutError, Select, Sender};
use lsp_server::{Connection, IoThreads, Message, ProtocolError, RequestId, ResponseError};
use lsp_types::{notification::{DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders, DidCloseTextDocument,
    DidCreateFiles, DidDeleteFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, Notification}, request::{Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Request, ResolveCompletionItem, Shutdown}, CompletionOptions, DefinitionOptions, DocumentSymbolOptions, FileOperationFilter, FileOperationPattern, FileOperationRegistrationOptions, HoverProviderCapability, InitializeParams, InitializeResult, OneOf, SaveOptions, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, WorkDoneProgressOptions, WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities};
use serde_json::json;
#[cfg(target_os = "linux")]
use nix;
use tracing::{error, info, warn};

use crate::{constants::{DEBUG_THREADS, EXTENSION_VERSION}, core::{file_mgr::FileMgr, odoo::SyncOdoo}, threads::{delayed_changes_process_thread, message_processor_thread_main, message_processor_thread_read, DelayedProcessingMessage}, S};

const THREAD_MAIN_COUNT: u16 = 1;
const THREAD_READ_COUNT: u16 = 1;

/**
 * Server handle connection between the client and the extension.
 * It can create a connection through io or tcp.
 *
 */
pub struct Server {
    pub connection: Option<Connection>,
    client_process_id: u32,
    io_threads: IoThreads,
    receivers_w_to_s: Vec<Receiver<Message>>,
    msg_id: i32,
    id_list: HashMap<RequestId, u16>, //map each request to its thread. firsts ids for main thread, nexts for read ones, last for delayed_process thread
    threads: Vec<JoinHandle<()>>,
    senders_s_to_main: Vec<Sender<Message>>, // specific channel to threads, to handle responses
    sender_s_to_main: Sender<Message>, //unique channel server to all main threads. Will handle new request message
    senders_s_to_read: Vec<Sender<Message>>, // specific channel to threads, to handle responses
    sender_s_to_read: Sender<Message>, //unique channel server to all read threads
    delayed_process_thread: JoinHandle<()>,
    sender_to_delayed_process: Sender<DelayedProcessingMessage>, //unique channel to delayed process thread
    sync_odoo: Arc<Mutex<SyncOdoo>>,
    interrupt_rebuild_boolean: Arc<AtomicBool>,
    terminate_rebuild_boolean: Arc<AtomicBool>,
}

#[derive(Debug)]

pub enum ServerError {
    ProtocolError(ProtocolError),
    Serialization(serde_json::Error),
    ServerError(String),
    ResponseError(ResponseError),
}

impl From<ProtocolError> for ServerError {
    fn from(error: ProtocolError) -> Self {
        ServerError::ProtocolError(error)
    }
}

impl From<serde_json::Error> for ServerError {
    fn from(error: serde_json::Error) -> Self {
        ServerError::Serialization(error)
    }
}

impl Server {

    pub fn new_tcp() -> Result<Self, Error> {
        match Connection::listen("127.0.0.1:2087") {
            Ok((conn, io_threads)) => {
                Ok(Server::init(conn, io_threads))
            },
            Err(e) => Err(e)
        }
    }

    pub fn new_stdio() -> Self {
        let (conn, io_threads) = Connection::stdio();
        Server::init(conn, io_threads)
    }

    fn init(conn: Connection, io_threads: IoThreads) -> Self {
        let mut threads = vec![];
        let sync_odoo = Arc::new(Mutex::new(SyncOdoo::new()));
        let interrupt_rebuild_boolean = sync_odoo.lock().unwrap().interrupt_rebuild.clone();
        let terminate_rebuild_boolean = sync_odoo.lock().unwrap().terminate_rebuild.clone();
        let mut receivers_w_to_s = vec![];
        let mut senders_s_to_main = vec![];
        let (sender_to_delayed_process, receiver_delayed_process) = crossbeam_channel::unbounded();
        let (generic_sender_s_to_main, generic_receiver_s_to_main) = crossbeam_channel::unbounded(); //unique channel to dispatch to any ready main thread
        for _ in 0..THREAD_MAIN_COUNT {
            let (sender_s_to_main, receiver_s_to_main) = crossbeam_channel::unbounded();
            let (sender_main_to_s, receiver_main_to_s) = crossbeam_channel::unbounded();
            senders_s_to_main.push(sender_s_to_main);
            receivers_w_to_s.push(receiver_main_to_s);

            threads.push({
                let sync_odoo = sync_odoo.clone();
                let generic_receiver_s_to_main = generic_receiver_s_to_main.clone();
                let sender_to_delayed_process = sender_to_delayed_process.clone();
                std::thread::spawn(move || {
                    message_processor_thread_main(sync_odoo, generic_receiver_s_to_main, sender_main_to_s.clone(), receiver_s_to_main.clone(), sender_to_delayed_process);
                })
            });
        }

        let mut senders_s_to_read = vec![];
        let (generic_sender_s_to_read, generic_receiver_s_to_read) = crossbeam_channel::unbounded(); //unique channel to dispatch to any ready read thread
        for _ in 0..THREAD_READ_COUNT {
            let (sender_s_to_read, receiver_s_to_read) = crossbeam_channel::unbounded();
            let (sender_read_to_s, receiver_read_to_s) = crossbeam_channel::unbounded();
            senders_s_to_read.push(sender_s_to_read);
            receivers_w_to_s.push(receiver_read_to_s);
            threads.push({
                let sync_odoo = sync_odoo.clone();
                let generic_receiver_s_to_read = generic_receiver_s_to_read.clone();
                let sender_to_delayed_process = sender_to_delayed_process.clone();
                std::thread::spawn(move || {
                    message_processor_thread_read(sync_odoo, generic_receiver_s_to_read.clone(), sender_read_to_s.clone(), receiver_s_to_read.clone(), sender_to_delayed_process);
                })
            });
        }

        let (_, receiver_s_to_delayed) = crossbeam_channel::unbounded();
        let (sender_delayed_to_s, receiver_delayed_to_s) = crossbeam_channel::unbounded();
        receivers_w_to_s.push(receiver_delayed_to_s);
        let so = sync_odoo.clone();
        let delayed_process_sender_to_delayed_process = sender_to_delayed_process.clone();
        let delayed_process_thread = std::thread::spawn(move || {
            delayed_changes_process_thread(sender_delayed_to_s, receiver_s_to_delayed, receiver_delayed_process, so, delayed_process_sender_to_delayed_process)
        });

        // let (sender_to_server, receiver_to_server) = crossbeam_channel::unbounded();
        // let (sender_from_server_reactive, receiver_from_server) = crossbeam_channel::unbounded();
        // server.add_receiver(receiver_to_server.clone());
        // for i in 0..THREAD_REACTIVE_COUNT {
        //     threads.push(std::thread::spawn(move || {
        //         message_processor_thread_reactive(sender_to_server.clone(), receiver_from_server.clone());
        //     }));
        // }
        Self {
            connection: Some(conn),
            client_process_id: 0,
            io_threads: io_threads,
            id_list: HashMap::new(),
            msg_id: 0,
            receivers_w_to_s: receivers_w_to_s,
            threads: threads,
            senders_s_to_main: senders_s_to_main,
            sender_s_to_main: generic_sender_s_to_main,
            senders_s_to_read: senders_s_to_read,
            sender_s_to_read: generic_sender_s_to_read,
            sender_to_delayed_process: sender_to_delayed_process,
            delayed_process_thread,
            sync_odoo: sync_odoo,
            interrupt_rebuild_boolean: interrupt_rebuild_boolean,
            terminate_rebuild_boolean
        }
    }


    pub fn initialize(&mut self) -> Result<(), ServerError> {
        info!("Waiting for a connection...");
        let (id, params) = self.connection.as_ref().unwrap().initialize_start()?;
        info!("Starting connection initialization");

        let initialize_params: InitializeParams = serde_json::from_value(params)?;
        {
            let mut sync_odoo = self.sync_odoo.lock().unwrap();
            sync_odoo.load_capabilities(&initialize_params.capabilities);
        }
        if let Some(initialize_params) = initialize_params.process_id {
            self.client_process_id = initialize_params;
        }
        if let Some(workspace_folders) = initialize_params.workspace_folders {
            let sync_odoo = self.sync_odoo.lock().unwrap();
            let file_mgr = sync_odoo.get_file_mgr();
            let mut file_mgr = file_mgr.borrow_mut();
            for added in workspace_folders.iter() {
                let path = FileMgr::uri2pathname(added.uri.as_str());
                file_mgr.add_workspace_folder(added.name.clone(), path);
            }
        } else if let Some( root_uri) = initialize_params.root_uri.as_ref() {
            let sync_odoo = self.sync_odoo.lock().unwrap();
            let file_mgr = sync_odoo.get_file_mgr();
            let mut file_mgr = file_mgr.borrow_mut();
            let path = FileMgr::uri2pathname(root_uri.as_str());
            file_mgr.add_workspace_folder(S!("_root"), path);
        }
        let initialize_data = InitializeResult {
            server_info: Some(ServerInfo {
                name: "Odoo Language Server".to_string(),
                version: Some(EXTENSION_VERSION.to_string())
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    open_close: Some(true),
                    will_save: None,
                    will_save_wait_until: None,
                    save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(SaveOptions{include_text: Some(false)})) //TODO could deactivate if set on 'afterDelay?
                })),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Right(DefinitionOptions{
                    work_done_progress_options: WorkDoneProgressOptions{
                        work_done_progress: Some(false)
                    }
                })),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![S!("."), S!(","), S!("'"), S!("\""), S!("(")]),
                    ..CompletionOptions::default()
                }),
                document_symbol_provider: Some(OneOf::Right(DocumentSymbolOptions{
                    label: Some(S!("Odoo")),
                    work_done_progress_options: WorkDoneProgressOptions{
                        work_done_progress: Some(false)
                    },
                })),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                        did_create: Some(FileOperationRegistrationOptions {
                            filters: vec![FileOperationFilter {
                                scheme: Some(S!("file")),
                                pattern: FileOperationPattern {
                                    glob: S!("**"),
                                    matches: None,
                                    options: None,
                                },
                            }]
                        }),
                        did_rename: Some(FileOperationRegistrationOptions {
                            filters: vec![
                                FileOperationFilter {
                                    scheme: Some(S!("file")),
                                    pattern: FileOperationPattern {
                                        glob: S!("**"),
                                        matches: None,
                                        options: None,
                                    },
                                }
                            ]
                        }),
                        did_delete: Some(FileOperationRegistrationOptions {
                            filters: vec![
                                FileOperationFilter {
                                    scheme: Some(S!("file")),
                                    pattern: FileOperationPattern {
                                        glob: S!("**"),
                                        matches: None,
                                        options: None,
                                    },
                                }
                            ]
                        }),
                        ..WorkspaceFileOperationsServerCapabilities::default()
                    })
                }),
                ..ServerCapabilities::default()
            }
        };

        self.connection.as_ref().unwrap().initialize_finish(id, serde_json::to_value(initialize_data).unwrap())?;
        let _ = self.connection.as_ref().unwrap().sender.send(Message::Notification(lsp_server::Notification {
            method: "$Odoo/setPid".to_string(),
            params: json!({
                "server_pid": std::process::id(),
            })
        }));
        info!("End of connection initalization.");
        self.sender_s_to_main.send(Message::Notification(lsp_server::Notification { method: S!("custom/server/register_capabilities"), params: serde_json::Value::Null })).unwrap();
        self.sender_s_to_main.send(Message::Notification(lsp_server::Notification { method: S!("custom/server/init"), params: serde_json::Value::Null })).unwrap();
        Ok(())
    }

    fn shutdown_threads(&mut self, message: &str){
        self.terminate_rebuild_boolean.store(true, std::sync::atomic::Ordering::SeqCst);
        let shutdown_notification = Message::Notification(lsp_server::Notification{
            method: Shutdown::METHOD.to_string(),
            params: serde_json::Value::Null,
        });
        for specific_sender in self.senders_s_to_main.iter() {
            self.sender_s_to_main.send(shutdown_notification.clone()).unwrap(); //sent as notification as we already handled the request for the client
            specific_sender.send(shutdown_notification.clone()).unwrap(); //send to specific channels too to close pending requests
        }
        for specific_sender in self.senders_s_to_read.iter() {
            self.sender_s_to_read.send(shutdown_notification.clone()).unwrap(); //sent as notification as we already handled the request for the client
            specific_sender.send(shutdown_notification.clone()).unwrap(); //send to specific channels too to close pending requests
        }
        info!(message);
    }

    pub fn run(mut self, client_pid: Option<u32>) -> bool {
        let mut select = Select::new();
        let receiver_clone = self.connection.as_ref().unwrap().receiver.clone();
        select.recv(&receiver_clone);
        let receivers_w_to_s_clone = self.receivers_w_to_s.clone();
        for receiver in receivers_w_to_s_clone.iter() {
            select.recv(receiver);
        };
        let (stop_sender, stop_receiver) = crossbeam_channel::unbounded();
        let mut pid_thread = None;
        let pid = client_pid.unwrap_or(self.client_process_id);
        if pid != 0 {
            pid_thread = Some(self.spawn_pid_thread(pid, stop_receiver));
        }
        let mut wait_exit_notification = false;
        let mut exit_no_error_code = false;
        loop {
            let index = select.ready();
            let res = if index == 0 {
                self.connection.as_ref().unwrap().receiver.try_recv()
            } else {
                self.receivers_w_to_s.get(index-1).unwrap().try_recv()
            };

            // If the operation turns out not to be ready, retry.
            if let Err(e) = res {
                //TODO what if non-0 index disconnect?
                if e.is_empty() {
                    continue;
                }
                if e.is_disconnected() {
                    warn!("Channel disconnected. Exiting program.");
                    break;
                }
            }
            let msg = res.unwrap();

            if DEBUG_THREADS {
                let msg_info = match msg {
                    Message::Request(ref r) => format!("Request: {} - {}", r.method, r.id),
                    Message::Response(ref r) => format!("Response: {}", r.id),
                    Message::Notification(ref n) => format!("Notification: {}", n.method),
                };
                info!("Got message from index {}, : {:?}", index, msg_info);
            }

            if index == 0 { //comes from client
                if let Message::Request(r) = &msg {
                    if r.method == "shutdown" {
                        let resp = lsp_server::Response::new_ok(r.id.clone(), ());
                        let _ = self.connection.as_ref().unwrap().sender.send(resp.into());
                        wait_exit_notification = true;
                        continue;
                    } else if wait_exit_notification {
                        error!("Got Request after a shutdown request. Ignoring it.")
                    }
                } else if let Message::Notification(n) = &msg {
                    if n.method == "exit" {
                        if !wait_exit_notification {
                            warn!("Got exit notification without a previous shutdown request. Exiting anyway.");
                            exit_no_error_code = false;
                        }
                        self.shutdown_threads("Got a client exit notification. Exiting.");
                        break;
                    }
                }
                self.dispatch(msg);
            } else { // comes from threads
                match msg {
                    Message::Request(mut r) => {
                        r.id = RequestId::from(self.msg_id);
                        self.msg_id += 1;
                        self.id_list.insert(r.id.clone(), index as u16);
                        self.connection.as_ref().unwrap().sender.send(Message::Request(r)).unwrap();
                    },
                    Message::Notification(n) => {
                        if n.method == Shutdown::METHOD{
                            self.shutdown_threads("Server-initiated shutdown request. Exiting");
                            break;
                        }
                        self.connection.as_ref().unwrap().sender.send(Message::Notification(n)).unwrap();
                    },
                    Message::Response(r) => {
                        self.connection.as_ref().unwrap().sender.send(Message::Response(r)).unwrap();
                    }
                }
            }
        }
        drop(select);
        drop(receiver_clone);
        let hook = panic::take_hook(); //drop sender stored in panic
        drop(hook);
        let _ = self.sender_to_delayed_process.send(DelayedProcessingMessage::EXIT);
        let _ = stop_sender.send(());
        self.connection = None; //drop connection before joining threads
        if let Some(pid_join_handle) = pid_thread {
            pid_join_handle.join().unwrap();
        }
        for thread in self.threads {
            thread.join().unwrap();
        }
        self.delayed_process_thread.join().unwrap();
        exit_no_error_code
    }

    /* address a message to the right thread. */
    fn dispatch(&mut self, msg: Message) {
        match msg {
            Message::Request(r) => {
                match r.method.as_str() {
                    HoverRequest::METHOD | GotoDefinition::METHOD => {
                        self.interrupt_rebuild_boolean.store(true, std::sync::atomic::Ordering::SeqCst);
                        if DEBUG_THREADS {
                            info!("Sending request to read thread : {} - {}", r.method, r.id);
                        }
                        self.sender_s_to_read.send(Message::Request(r)).unwrap();
                    },
                    Completion::METHOD => {
                        self.interrupt_rebuild_boolean.store(true, std::sync::atomic::Ordering::SeqCst);
                        if DEBUG_THREADS {
                            info!("Sending request to main thread : {} - {}", r.method, r.id);
                        }
                        self.sender_s_to_main.send(Message::Request(r)).unwrap();
                    },
                    DocumentSymbolRequest::METHOD => {
                        if DEBUG_THREADS {
                            info!("Sending request to read thread : {} - {}", r.method, r.id);
                        }
                        self.sender_s_to_read.send(Message::Request(r)).unwrap();
                    }
                    ResolveCompletionItem::METHOD => {
                        info!("Got ignored CompletionItem/resolve")
                    }
                    _ => {panic!("Not handled Request Id: {}", r.method)}
                }
            },
            Message::Response(r) => {
                let thread_id = self.id_list.get(&r.id);
                if let Some(thread_id) = thread_id {
                    if *thread_id == 0_u16 {
                        panic!("thread_id can't be equal to 0. Client can't respond to itself");
                    } else {
                        let mut t_id = thread_id - 1;
                        if t_id < THREAD_MAIN_COUNT {
                            if DEBUG_THREADS {
                                info!("Sending response to main thread : {}", r.id);
                            }
                            self.senders_s_to_main.get(t_id as usize).unwrap().send(Message::Response(r)).unwrap();
                            return;
                        }
                        t_id -= THREAD_MAIN_COUNT;
                        if t_id < THREAD_READ_COUNT {
                            if DEBUG_THREADS {
                                info!("Sending response to read thread : {}", r.id);
                            }
                            self.senders_s_to_read.get(t_id as usize).unwrap().send(Message::Response(r)).unwrap();
                            return;
                        }
                        // t_id -= THREAD_READ_COUNT;
                        // if t_id < THREAD_REACTIVE_COUNT {
                        //     self.senders_s_to_react.get(t_id as usize).unwrap().send(msg);
                        //     return;
                        // }
                        panic!("invalid thread id");
                    }
                } else {
                    panic!("Got a response for an unknown request: {:?}", r);
                }
            },
            Message::Notification(n) => {
                match n.method.as_str() {
                    DidOpenTextDocument::METHOD | DidChangeConfiguration::METHOD | DidChangeWorkspaceFolders::METHOD |
                    DidChangeTextDocument::METHOD | DidCloseTextDocument::METHOD | DidSaveTextDocument::METHOD |
                    DidRenameFiles::METHOD | DidCreateFiles::METHOD | DidChangeWatchedFiles::METHOD | DidDeleteFiles::METHOD => {
                        if DEBUG_THREADS {
                            info!("Sending notification to main thread : {}", n.method);
                        }
                        self.interrupt_rebuild_boolean.store(true, std::sync::atomic::Ordering::SeqCst);
                        self.sender_s_to_main.send(Message::Notification(n)).unwrap();
                    }
                    _ => {
                        if n.method.starts_with("$/") {
                            warn!("Not handled message id: {}", n.method);
                        } else {
                            error!("Not handled Notification Id: {}", n.method)
                        }
                    }
                }
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn spawn_pid_thread(&self, pid: u32, stop_channel: Receiver<()>) -> JoinHandle<()> {
        use std::process::exit;
        info!("Got PID to watch: {}", pid);

        std::thread::spawn(move || {
            let pid = nix::unistd::Pid::from_raw(pid as i32);
            loop {
                if stop_channel.recv_timeout(std::time::Duration::from_millis(10)).is_ok() {
                    break;
                }

                match nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)) {
                    Ok(nix::sys::wait::WaitStatus::Exited(_, status)) => {
                        warn!("Process {} exited with status {} - killing extension in 10 secs", pid, status);
                        std::thread::sleep(std::time::Duration::from_secs(10));
                        exit(1);
                    }
                    Ok(nix::sys::wait::WaitStatus::Signaled(_, signal, _)) => {
                        warn!("Process {} was killed by signal {:?} - killing extension in 10 secs", pid, signal);
                        std::thread::sleep(std::time::Duration::from_secs(10));
                        exit(1);
                    }
                    Ok(nix::sys::wait::WaitStatus::StillAlive) => {
                        // Process is still running, continue waiting
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    Ok(_) => {
                        // Other wait statuses can be ignored for this purpose
                    }
                    Err(err) => {
                        eprintln!("Error waiting for process {}: {}", pid, err);
                        break;
                    }
                }
            }
        })
    }

    #[cfg(target_os = "windows")]
    fn spawn_pid_thread(&self, pid: u32, stop_channel: Receiver<()>) -> JoinHandle<()> {
        use std::process::exit;
        use winapi::um::processthreadsapi::OpenProcess;
        use winapi::um::winnt::PROCESS_QUERY_INFORMATION;
        use winapi::um::winbase::WAIT_OBJECT_0;
        use winapi::um::synchapi::WaitForSingleObject;
        use winapi::um::handleapi::CloseHandle;
        info!("Got PID to watch: {}", pid);

        std::thread::spawn(move || {
            unsafe {
                let process_handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
                if process_handle.is_null() {
                    error!("Failed to open process with PID: {}", pid);
                    return;
                }

                loop {
                    if stop_channel.recv_timeout(std::time::Duration::from_millis(10)).is_ok() {
                        break;
                    }
                    let wait_result = WaitForSingleObject(process_handle, 1000);

                    match wait_result {
                        WAIT_OBJECT_0 => {
                            info!("Process {} exited - killing extension in 10 secs", pid);
                            std::thread::sleep(std::time::Duration::from_secs(10));
                            CloseHandle(process_handle);
                            exit(1);
                        }
                        _ => {
                            // Process is still running, continue waiting
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            }
        })
    }
}