use std::{collections::HashMap, io::Error, panic, sync::{Arc, Mutex}, thread::JoinHandle};

use crossbeam_channel::{Receiver, Select, Sender};
use lsp_server::{Connection, IoThreads, Message, ProtocolError, RequestId, ResponseError};
use lsp_types::{notification::{DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders, DidCloseTextDocument,
    DidCreateFiles, DidDeleteFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, Notification},
    request::{Completion, GotoDefinition, HoverRequest, Request, ResolveCompletionItem, Shutdown}, CompletionOptions, DefinitionOptions,
    FileOperationFilter, FileOperationPattern, FileOperationRegistrationOptions, HoverProviderCapability, InitializeParams, InitializeResult,
    OneOf, SaveOptions, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    WorkDoneProgressOptions, WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities};
use serde_json::json;
#[cfg(target_os = "linux")]
use nix;
use tracing::{error, info, warn};

use crate::{core::{file_mgr::FileMgr, odoo::SyncOdoo}, threads::{delayed_changes_process_thread, message_processor_thread_main, message_processor_thread_read, DelayedProcessingMessage}, S};

const THREAD_MAIN_COUNT: u16 = 1;
const THREAD_READ_COUNT: u16 = 1;
const THREAD_REACTIVE_COUNT: u16 = 1;

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
    sender_s_to_delayed: Sender<Message>, //unique channel server to delayed_process_thread
    sender_to_delayed_process: Sender<DelayedProcessingMessage>, //unique channel to delayed process thread
    sync_odoo: Arc<Mutex<SyncOdoo>>,
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
        let mut receivers_w_to_s = vec![];
        let mut senders_s_to_main = vec![];
        let (sender_to_delayed_process, receiver_delayed_process) = crossbeam_channel::unbounded();
        let (generic_sender_s_to_main, generic_receiver_s_to_main) = crossbeam_channel::unbounded(); //unique channel to dispatch to any ready main thread
        for i in 0..THREAD_MAIN_COUNT {
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
        for i in 0..THREAD_READ_COUNT {
            let (sender_s_to_read, receiver_s_to_read) = crossbeam_channel::unbounded();
            let (sender_read_to_s, receiver_read_to_s) = crossbeam_channel::unbounded();
            senders_s_to_read.push(sender_s_to_read);
            receivers_w_to_s.push(receiver_read_to_s);
            threads.push({
                let sync_odoo = sync_odoo.clone();
                let generic_receiver_s_to_read = generic_receiver_s_to_read.clone();
                std::thread::spawn(move || {
                    message_processor_thread_read(sync_odoo, generic_receiver_s_to_read.clone(), sender_read_to_s.clone(), receiver_s_to_read.clone());
                })
            });
        }

        let (sender_s_to_delayed, receiver_s_to_delayed) = crossbeam_channel::unbounded();
        let (sender_delayed_to_s, receiver_delayed_to_s) = crossbeam_channel::unbounded();
        receivers_w_to_s.push(receiver_delayed_to_s);
        let so = sync_odoo.clone();
        let delayed_process_thread = std::thread::spawn(move || {
            delayed_changes_process_thread(sender_delayed_to_s, receiver_s_to_delayed, receiver_delayed_process, so)
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
            sender_s_to_delayed: sender_s_to_delayed,
            sender_to_delayed_process: sender_to_delayed_process,
            delayed_process_thread,
            sync_odoo: sync_odoo
        }
    }


    pub fn initialize(&mut self) -> Result<(), ServerError> {
        info!("Waiting for a connection...");
        let (id, params) = self.connection.as_ref().unwrap().initialize_start()?;
        info!("Starting connection initialization");

        let initialize_params: InitializeParams = serde_json::from_value(params)?;
        if let Some(initialize_params) = initialize_params.process_id {
            self.client_process_id = initialize_params;
        }
        //TODO if no workspace_folders, use root_uri
        if let Some(workspace_folders) = initialize_params.workspace_folders {
            let mut sync_odoo = self.sync_odoo.lock().unwrap();
            let file_mgr = sync_odoo.get_file_mgr();
            let mut file_mgr = file_mgr.borrow_mut();
            for added in workspace_folders.iter() {
                let path = FileMgr::uri2pathname(added.uri.as_str());
                file_mgr.add_workspace_folder(path);
            }
        }
        let initialize_data = InitializeResult {
            server_info: Some(ServerInfo {
                name: "Odoo Language Server".to_string(),
                version: Some("0.2.0".to_string())
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    open_close: Some(true),
                    will_save: None,
                    will_save_wait_until: None,
                    save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(SaveOptions{include_text: Some(true)})) //TODO could deactivate if set on 'afterDelay?
                })),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Right(DefinitionOptions{
                    work_done_progress_options: WorkDoneProgressOptions{
                        work_done_progress: Some(false)
                    }
                })),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    trigger_characters: Some(vec![S!("."), S!(","), S!("'"), S!("\"")]),
                    ..CompletionOptions::default()
                }),
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
            },
            ..Default::default()
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

    pub fn run(mut self, client_pid: Option<u32>) {
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
            let sender_to_main = self.sender_s_to_main.clone();
            pid_thread = Some(self.spawn_pid_thread(pid, sender_to_main, stop_receiver));
        }
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

            if index == 0 { //comes from client
                if let Message::Request(r) = &msg {
                    if self.connection.as_ref().unwrap().handle_shutdown(r).unwrap_or(false) {
                        for _ in 0..self.senders_s_to_main.len() {
                            self.sender_s_to_main.send(Message::Notification(lsp_server::Notification{
                                method: Shutdown::METHOD.to_string(),
                                params: serde_json::Value::Null,
                            })).unwrap(); //sent as notification as we already handled the request for the client
                        }
                        for _ in 0..self.senders_s_to_read.len() {
                            self.sender_s_to_read.send(Message::Notification(lsp_server::Notification{
                                method: Shutdown::METHOD.to_string(),
                                params: serde_json::Value::Null,
                            })).unwrap(); //sent as notification as we already handled the request for the client
                        }
                        info!("Got shutdown request. Exiting.");
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
        self.io_threads.join().unwrap();
        self.delayed_process_thread.join().unwrap();
    }

    /* address a message to the right thread. */
    fn dispatch(&mut self, msg: Message) {
        match msg {
            Message::Request(r) => {
                match r.method.as_str() {
                    HoverRequest::METHOD | GotoDefinition::METHOD | Completion::METHOD => {
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
                    if *thread_id == 0 as u16 {
                        panic!("thread_id can't be equal to 0. Client can't respond to itself");
                    } else {
                        let mut t_id = thread_id.clone() - 1;
                        if t_id < THREAD_MAIN_COUNT {
                            self.senders_s_to_main.get(t_id as usize).unwrap().send(Message::Response(r)).unwrap();
                            return;
                        }
                        t_id -= THREAD_MAIN_COUNT;
                        if t_id < THREAD_READ_COUNT {
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

    #[cfg(target_os = "linux")]
    fn spawn_pid_thread(&self, pid: u32, sender_s_to_main: Sender<Message>, stop_channel: Receiver<()>) -> JoinHandle<()> {
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
                        // Le processus est toujours en cours d'exécution, attendez un peu avant de réessayer
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    Ok(_) => {
                        // Autres statuts peuvent être ignorés
                    }
                    Err(err) => {
                        eprintln!("Error waiting for process {}: {}", pid, err);
                        break;
                    }
                }
            }
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn spawn_pid_thread(&self, pid: u32, sender_s_to_main: Sender<Message>, stop_channel: Receiver<()>) -> JoinHandle<()> {
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
                    let wait_result = WaitForSingleObject(process_handle, 1000); // Attendre 1 seconde

                    match wait_result {
                        WAIT_OBJECT_0 => {
                            info!("Process {} exited - killing extension in 10 secs", pid);
                            std::thread::sleep(std::time::Duration::from_secs(10));
                            CloseHandle(process_handle);
                            exit(1);
                        }
                        _ => {
                            // Le processus est toujours en cours d'exécution, attendez un peu avant de réessayer
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            }
        })
    }
}