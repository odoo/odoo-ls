use std::{collections::HashMap, io::Error, sync::{mpsc::RecvTimeoutError, Arc, Mutex, RwLock}, thread::JoinHandle};

use crossbeam_channel::{Receiver, RecvError, Select, Sender};
use lsp_server::{Connection, IoThreads, Message, ProtocolError, RequestId, Response, ResponseError};
use lsp_types::{notification::{DidChangeConfiguration, DidChangeTextDocument, DidChangeWorkspaceFolders, DidCloseTextDocument, DidCreateFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, Notification}, request::{Completion, GotoDefinition, HoverRequest, RegisterCapability, Request, Shutdown}, CompletionOptions, DefinitionOptions, DidChangeWatchedFilesRegistrationOptions, FileOperationFilter, FileOperationPattern, FileOperationRegistrationOptions, FileSystemWatcher, GlobPattern, HoverProviderCapability, InitializeParams, InitializeResult, MessageType, OneOf, Registration, RegistrationParams, SaveOptions, ServerCapabilities, ServerInfo, TextDocumentChangeRegistrationOptions, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, WatchKind, WorkDoneProgressOptions, WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities};
use serde_json::to_value;

use crate::{core::{file_mgr::FileMgr, odoo::{Odoo, SyncOdoo}}, threads::{message_processor_thread_main, message_processor_thread_read}, S};

const THREAD_MAIN_COUNT: u16 = 1;
const THREAD_READ_COUNT: u16 = 1;
const THREAD_REACTIVE_COUNT: u16 = 1;

/**
 * Server handle connection between the client and the extension.
 * It can create a connection through io or tcp.
 * 
 */
pub struct Server {
    connection: Connection,
    io_threads: IoThreads,
    receivers_w_to_s: Vec<Receiver<Message>>,
    msg_id: i32,
    id_list: HashMap<RequestId, u16>,
    threads: Vec<JoinHandle<()>>,
    senders_s_to_main: Vec<Sender<Message>>, // specific channel to threads, to handle responses
    sender_s_to_main: Sender<Message>, //unique channel server to all main threads. Will handle new request message
    senders_s_to_read: Vec<Sender<Message>>, // specific channel to threads, to handle responses
    sender_s_to_read: Sender<Message>, //unique channel server to all read threads
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
        let (generic_sender_s_to_main, generic_receiver_s_to_main) = crossbeam_channel::unbounded(); //unique channel to dispatch to any ready main thread
        for i in 0..THREAD_MAIN_COUNT {
            let (sender_s_to_main, receiver_s_to_main) = crossbeam_channel::unbounded();
            let (sender_main_to_s, receiver_main_to_s) = crossbeam_channel::unbounded();
            senders_s_to_main.push(sender_s_to_main);
            receivers_w_to_s.push(receiver_main_to_s);

            threads.push({
                let sync_odoo = sync_odoo.clone();
                let generic_receiver_s_to_main = generic_receiver_s_to_main.clone();
                std::thread::spawn(move || {
                    message_processor_thread_main(sync_odoo, generic_receiver_s_to_main, sender_main_to_s.clone(), receiver_s_to_main.clone());
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

        // let (sender_to_server, receiver_to_server) = crossbeam_channel::unbounded();
        // let (sender_from_server_reactive, receiver_from_server) = crossbeam_channel::unbounded();
        // server.add_receiver(receiver_to_server.clone());
        // for i in 0..THREAD_REACTIVE_COUNT {
        //     threads.push(std::thread::spawn(move || {
        //         message_processor_thread_reactive(sender_to_server.clone(), receiver_from_server.clone());
        //     }));
        // }
        Self {
            connection: conn,
            io_threads: io_threads,
            id_list: HashMap::new(),
            msg_id: 0,
            receivers_w_to_s: receivers_w_to_s,
            threads: threads,
            senders_s_to_main: senders_s_to_main,
            sender_s_to_main: generic_sender_s_to_main,
            senders_s_to_read: senders_s_to_read,
            sender_s_to_read: generic_sender_s_to_read,
            sync_odoo: sync_odoo
        }
    }


    pub fn initialize(&mut self) -> Result<(), ServerError> {
        println!("Waiting for a connection...");
        let (id, params) = self.connection.initialize_start()?;
        println!("Starting connection initialization");

        let initialize_params: InitializeParams = serde_json::from_value(params)?;
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
                        ..WorkspaceFileOperationsServerCapabilities::default()
                    })
                }),
                ..ServerCapabilities::default()
            },
            ..Default::default()
        };

        self.connection.initialize_finish(id, serde_json::to_value(initialize_data).unwrap())?;
        println!("End of connection initalization.");
        self.sender_s_to_main.send(Message::Notification(lsp_server::Notification { method: S!("custom/server/register_capabilities"), params: serde_json::Value::Null })).unwrap();
        self.sender_s_to_main.send(Message::Notification(lsp_server::Notification { method: S!("custom/server/init"), params: serde_json::Value::Null })).unwrap();
        Ok(())
    }

    pub fn run(mut self) {
        let mut select = Select::new();
        let receiver_clone = self.connection.receiver.clone();
        select.recv(&receiver_clone);
        let receivers_w_to_s_clone = self.receivers_w_to_s.clone();
        for receiver in receivers_w_to_s_clone.iter() {
            select.recv(receiver);
        }
        loop {
            let index = select.ready();
            let res = if index == 0 {
                self.connection.receiver.try_recv()
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
                    println!("Channel disconnected. Exiting program.");
                    break;
                }
            }
            let msg = res.unwrap();

            if index == 0 { //comes from client
                if let Message::Request(r) = &msg {
                    if r.method == Shutdown::METHOD {
                        self.connection.sender.send(Message::Response(Response{
                            id: r.id.clone(),
                            result: Some(serde_json::Value::Null),
                            error: None,
                        })).unwrap();
                        println!("Got shutdown request. Exiting.");
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
                        self.connection.sender.send(Message::Request(r)).unwrap();
                    },
                    Message::Notification(n) => {
                        self.connection.sender.send(Message::Notification(n)).unwrap();
                    },
                    Message::Response(r) => {
                        self.connection.sender.send(Message::Response(r)).unwrap();
                    }
                }
            }
        }
        self.io_threads.join().unwrap();
    }

    /* address a message to the right thread. */
    fn dispatch(&mut self, msg: Message) {
        match msg {
            Message::Request(r) => {
                match r.method.as_str() {
                    HoverRequest::METHOD | GotoDefinition::METHOD | Completion::METHOD => {
                        self.sender_s_to_read.send(Message::Request(r)).unwrap();
                    }
                    _ => {panic!("Not handled Message Id: {}", r.method)}
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
                    DidRenameFiles::METHOD | DidCreateFiles::METHOD => {
                        self.sender_s_to_main.send(Message::Notification(n)).unwrap();
                    }
                    _ => {
                        if n.method.starts_with("$/") {
                            println!("Not handled message id: {}", n.method);
                        } else {
                            panic!("Not handled Message Id: {}", n.method)
                        }
                    }
                }
            }
        }
    }
}