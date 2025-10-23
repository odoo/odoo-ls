use std::{collections::VecDeque, path::PathBuf, sync::{Arc, Mutex}, time::Instant};

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use lsp_server::{Message, RequestId, Response, ResponseError};
use lsp_types::{notification::{DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders,
    DidCloseTextDocument, DidCreateFiles, DidDeleteFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, LogMessage,
    Notification, ShowMessage}, request::{Completion, DocumentSymbolRequest, GotoDefinition, GotoTypeDefinitionResponse, HoverRequest, References, Request, Shutdown}, CompletionResponse, DocumentSymbolResponse, Hover, Location, LogMessageParams, MessageType, ShowMessageParams};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};
use crate::{constants::MAX_WATCHED_FILES_UPDATES_BEFORE_RESTART, create_session};

use crate::{core::{file_mgr::NoqaInfo, odoo::{Odoo, SyncOdoo}}, server::ServerError, utils::PathSanitizer, S};

pub struct SessionInfo<'a> {
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    pub sync_odoo: &'a mut SyncOdoo,
    delayed_process_sender: Option<Sender<DelayedProcessingMessage>>,
    pub noqas_stack: Vec<NoqaInfo>,
    pub current_noqa: NoqaInfo,
}

impl <'a> SessionInfo<'a> {
    pub fn log_message(&self, msg_type: MessageType, msg: String) {
        self.sender.send(
            Message::Notification(lsp_server::Notification{
                method: LogMessage::METHOD.to_string(),
                params: serde_json::to_value(&LogMessageParams{typ: msg_type, message: msg}).unwrap()
            })
        ).unwrap();
    }

    pub fn send_notification<T: Serialize>(&self, method: &str, params: T) {
        let param = serde_json::to_value(params);
        let Ok(param) = param else {
            error!("Unable to serialize parameters for method {}", method);
            return;
        };
        self.sender.send(
            Message::Notification(lsp_server::Notification{
                method: method.to_string(),
                params: param
            })
        ).unwrap();
    }

    pub fn show_message(&self, msg_type: MessageType, msg: String) {
        self.sender.send(
            Message::Notification(lsp_server::Notification{
                method: ShowMessage::METHOD.to_string(),
                params: serde_json::to_value(&ShowMessageParams{typ: msg_type, message: msg}).unwrap()
            })
        ).unwrap();
    }

    pub fn send_request<T: Serialize, U: DeserializeOwned>(&self, method: &str, params: T) -> Result<Option<U>, ServerError> {
        let param = serde_json::to_value(params)?;
        self.sender.send(Message::Request(lsp_server::Request{
                id: RequestId::from(0), //will be set by Server
                method: S!(method),
                params: param
        })).unwrap();
        match self.receiver.recv() {
            Ok(Message::Response(r)) => {
                //We can't check the response ID because it is set by Server. This is the reason Server must check that the id is correct.
                if let Some(resp_error) = r.error {
                    error!("Got error for response of {}: {}", method, resp_error.message);
                    return Err(ServerError::ResponseError(resp_error));
                } else {
                    match r.result {
                        Some(res) => {
                            let serialized = serde_json::from_value(res);
                            match serialized {
                                Ok(content) => {Ok(content)},
                                Err(e) => Err(ServerError::Serialization(e))
                            }
                        },
                        None => {return Ok(None)},
                    }
                }
            },
            Ok(Message::Request(r)) => {
                if r.method == Shutdown::METHOD {
                    return Err(ServerError::ServerError("Server is shutting down, cancelling request".to_string()));
                }
                return Err(ServerError::ServerError("Not a Response.".to_string()))
            }
            Ok(_) => return Err(ServerError::ServerError("Not a Response.".to_string())),
            Err(_) => return Err(ServerError::ServerError("Server disconnected".to_string())),
        }
    }

    /*
    * Request an update of the file in the index.
    * path: path of the file
    * forced_delay: indicate that we want to force a delay
     */
    pub fn request_update_file_index(session: &mut SessionInfo, path: &PathBuf, forced_delay: bool) {
        if forced_delay {
            session.sync_odoo.watched_file_updates += 1;
        }
        if session.sync_odoo.watched_file_updates > MAX_WATCHED_FILES_UPDATES_BEFORE_RESTART {
            let _ = session.delayed_process_sender.as_ref().unwrap().send(DelayedProcessingMessage::RESTART);
            return;
        }
        let _ = SyncOdoo::_unload_path(session, &path, false);
        Odoo::search_symbols_to_rebuild(session, &path.sanitize());
        if (!forced_delay || session.delayed_process_sender.is_none()) && !session.sync_odoo.need_rebuild {
            if session.sync_odoo.get_rebuild_queue_size() < 10 {
                SyncOdoo::process_rebuilds(session, false);
                return;
            }
        }
        let _ = session.delayed_process_sender.as_ref().unwrap().send(DelayedProcessingMessage::PROCESS(std::time::Instant::now()));
    }

    pub fn request_delayed_rebuild(&self) {
        if let Some(sender) = &self.delayed_process_sender {
            let _ = sender.send(DelayedProcessingMessage::PROCESS(std::time::Instant::now()));
        }
    }

    pub fn update_delay_thread_delay_duration(&self, delay_ms: u64) {
        if let Some(sender) = &self.delayed_process_sender {
            let _ = sender.send(DelayedProcessingMessage::UPDATE_DELAY(delay_ms));
        }
    }

    /* use it for test or tools, that do not need to connect to the server, and only want a fake session to use SyncOdoo */
    pub fn new_from_custom_channel(sender: Sender<Message>, receiver: Receiver<Message>, sync_odoo: &'a mut SyncOdoo) -> Self {
        Self {
            sender,
            receiver,
            sync_odoo,
            delayed_process_sender: None,
            noqas_stack: vec![],
            current_noqa: NoqaInfo::None,
        }
    }
}

fn to_value<T: Serialize + std::fmt::Debug>(result: Result<Option<T>, ResponseError>) -> (Option<Value>, Option<ResponseError>) {
    let value = match &result {
        Ok(Some(r)) => Some(serde_json::json!(r)),
        Ok(None) => Some(serde_json::Value::Null),
        Err(_) => None
    };
    let mut error = None;
    if result.is_err() {
        error = Some(result.unwrap_err());
    }
    (value, error)
}

#[derive(Debug)]
pub struct UpdateFileIndexData {
    pub path: PathBuf,
    pub time: Instant,
    pub forced_delay: bool,
}

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum DelayedProcessingMessage {
    PROCESS(Instant), //Process rebuilds after delay
    RESTART, //Ask to restart server when no busy or git locked
    UPDATE_DELAY(u64), //update the delay time
    EXIT, //exit the thread
}

fn restart_server(sync_odoo: &Arc<Mutex<SyncOdoo>>, sender_session: &Sender<Message>, receiver_session: &Receiver<Message>, delayed_process_sender: &Sender<DelayedProcessingMessage>) {
    let message = "Too many requests, possible change of branch, restarting Odoo LS";
    info!(message);
    {
        let session = SessionInfo{
            sender: sender_session.clone(),
            receiver: receiver_session.clone(),
            sync_odoo: &mut sync_odoo.lock().unwrap(),
            delayed_process_sender: Some(delayed_process_sender.clone()),
            noqas_stack: vec![],
            current_noqa: NoqaInfo::None,
        };
        session.send_notification("$Odoo/restartNeeded", ());
    }
}

fn notify_git_lock(sync_odoo: &Arc<Mutex<SyncOdoo>>, sender_session: &Sender<Message>, receiver_session: &Receiver<Message>, delayed_process_sender: &Sender<DelayedProcessingMessage>, status: &str) {
    {
        let session = SessionInfo{
            sender: sender_session.clone(),
            receiver: receiver_session.clone(),
            sync_odoo: &mut sync_odoo.lock().unwrap(),
            delayed_process_sender: Some(delayed_process_sender.clone()),
            noqas_stack: vec![],
            current_noqa: NoqaInfo::None,
        };
        error!("Git index lock detected, notifying client: {}", status);
        session.send_notification("$Odoo/loadingStatusUpdate", status);
    }
}

pub fn delayed_changes_process_thread(sender_session: Sender<Message>, receiver_session: Receiver<Message>, receiver: Receiver<DelayedProcessingMessage>, sync_odoo: Arc<Mutex<SyncOdoo>>, delayed_process_sender: Sender<DelayedProcessingMessage>) {
    const MAX_DELAY: u64 = 15000;
    const MIN_DELAY: u64 = 1000;
    let mut config_delay = std::time::Duration::from_millis(std::cmp::max(MIN_DELAY, std::cmp::min(sync_odoo.lock().unwrap().config.auto_refresh_delay, MAX_DELAY)));
    let mut messages = VecDeque::new();
    let mut to_wait = config_delay.clone();
    let mut got_process = false;
    let mut waiting_restart = false;
    loop {
        let msg = receiver.recv_timeout(to_wait);
        // Check if immediate reaction is needed, else add the message to the list
        match msg {
            Ok(DelayedProcessingMessage::RESTART) => {
                let main_entry_path = sync_odoo.lock().unwrap().config.odoo_path.as_ref().cloned(); //avoid keeping lock
                if let Some(main_entry_path) = main_entry_path {
                    let index_lock_path = PathBuf::from(main_entry_path).join(".git").join("index.lock");
                    let mut notified = false;
                    while index_lock_path.exists(){
                        if !notified {
                            notify_git_lock(&sync_odoo, &sender_session, &receiver_session, &delayed_process_sender, "git_locked");
                            notified = true;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    if notified {
                        notify_git_lock(&sync_odoo, &sender_session, &receiver_session, &delayed_process_sender, "stop");
                    }
                }
                if !waiting_restart {
                    waiting_restart = true;
                    restart_server(&sync_odoo, &sender_session, &receiver_session, &delayed_process_sender);
                }
                continue;
            }
            Ok(DelayedProcessingMessage::PROCESS(p)) => {
                got_process = true;
                messages.push_back(DelayedProcessingMessage::PROCESS(p));
                to_wait = p + config_delay - std::time::Instant::now();
                continue;
            }
            Ok(DelayedProcessingMessage::UPDATE_DELAY(d)) => {
                config_delay = std::time::Duration::from_millis(std::cmp::max(MIN_DELAY, std::cmp::min(d, MAX_DELAY)));
                to_wait = config_delay.clone();
                continue;
            }
            Ok(DelayedProcessingMessage::EXIT) => {
                return;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // inactivity timeout, process messages
            },
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                error!("Delayed processing channel disconnected, exiting thread");
                return;
            },
        }
        if waiting_restart {
            continue;
        }
        if got_process{
            got_process = false;
            let mut session = SessionInfo{
                sender: sender_session.clone(),
                receiver: receiver_session.clone(),
                sync_odoo: &mut sync_odoo.lock().unwrap(),
                delayed_process_sender: Some(delayed_process_sender.clone()),
                noqas_stack: vec![],
                current_noqa: NoqaInfo::None,
            };
            info!("Processing delayed file changes...");
            SyncOdoo::process_rebuilds(&mut session, false);
        }
    }
}

pub fn message_processor_thread_main(sync_odoo: Arc<Mutex<SyncOdoo>>, generic_receiver: Receiver<Message>, sender: Sender<Message>, receiver: Receiver<Message>, delayed_process_sender: Sender<DelayedProcessingMessage>) {
    let mut buffer = VecDeque::new();
    loop {
        // Drain all available messages into buffer
        loop {
            let maybe_msg = generic_receiver.try_recv();
            match maybe_msg {
                Ok(msg) => {
                    // Check for shutdown
                    if matches!(&msg, Message::Notification(n) if n.method.as_str() == Shutdown::METHOD) {
                        warn!("Main thread - got shutdown.");
                        return;
                    }
                    buffer.push_back(msg);
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    error!("Generic channel disconnected, exiting thread");
                    return;
                }
            }
        }
        // If buffer is empty, block for next message so we do not busy wait
        if buffer.is_empty() {
            match generic_receiver.recv() {
                Ok(msg) => {
                    // Check for shutdown
                    if matches!(&msg, Message::Notification(n) if n.method.as_str() == Shutdown::METHOD) {
                        warn!("Main thread - got shutdown.");
                        return;
                    }
                    buffer.push_back(msg);
                },
                Err(_) => {
                    error!("Got an RecvError, exiting thread");
                    break;
                }
            }
        }
        // Process buffered messages
        if let Some(msg) = buffer.pop_front() {
            match msg {
                Message::Request(r) => {
                    let (value, error) = match r.method.as_str() {
                        HoverRequest::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            to_value::<Hover>(Odoo::handle_hover(&mut session, serde_json::from_value(r.params).unwrap()))
                        },
                        GotoDefinition::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            to_value::<GotoTypeDefinitionResponse>(Odoo::handle_goto_definition(&mut session, serde_json::from_value(r.params).unwrap()))
                        },
                        References::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            to_value::<Vec<Location>>(Odoo::handle_references(&mut session, serde_json::from_value(r.params).unwrap()))
                        },
                        DocumentSymbolRequest::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            to_value::<DocumentSymbolResponse>(Odoo::handle_document_symbols(&mut session, serde_json::from_value(r.params).unwrap()))
                        },
                        Completion::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            to_value::<CompletionResponse>(Odoo::handle_autocomplete(&mut session, serde_json::from_value(r.params).unwrap()))
                        },
                        _ => {error!("Request not handled by main thread: {}", r.method); (None, Some(ResponseError{
                            code: 1,
                            message: S!("Request not handled by the server"),
                            data: None
                        }))}
                    };
                    sender.send(Message::Response(Response { id: r.id, result: value, error: error })).unwrap();
                },
                Message::Notification(n) => {
                    match n.method.as_str() {
                        DidOpenTextDocument::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            Odoo::handle_did_open(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidChangeConfiguration::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            Odoo::handle_did_change_configuration(&mut session, serde_json::from_value(n.params).unwrap())
                        }
                        DidChangeWorkspaceFolders::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            Odoo::handle_did_change_workspace_folders(&mut session, serde_json::from_value(n.params).unwrap())
                        }
                        DidChangeTextDocument::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            Odoo::handle_did_change(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidCloseTextDocument::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            Odoo::handle_did_close(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidSaveTextDocument::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, false);
                            Odoo::handle_did_save(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidRenameFiles::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            Odoo::handle_did_rename(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidCreateFiles::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            Odoo::handle_did_create(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidDeleteFiles::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            Odoo::handle_did_delete(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        DidChangeWatchedFiles::METHOD => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            SyncOdoo::process_rebuilds(&mut session, true);
                            Odoo::handle_did_change_watched_files(&mut session, serde_json::from_value(n.params).unwrap());
                        }
                        "custom/server/register_capabilities" => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            Odoo::register_capabilities(&mut session);
                        }
                        "custom/server/init" => {
                            let mut session = create_session!(sender, receiver, sync_odoo, delayed_process_sender);
                            Odoo::init(&mut session);
                        }
                        Shutdown::METHOD => { warn!("Main thread - got shutdown."); return;} // should be already caught
                        _ => {error!("Notification not handled by main thread: {}", n.method)}
                    }
                },
                Message::Response(_) => {
                    error!("Error: Responses should not arrives in generic channel. Exiting thread");
                    return;
                }
            }
        }
    }
}

#[macro_export]
macro_rules! create_session {
    ($sender:expr, $receiver:expr, $sync_odoo:expr, $delayed_sender:expr) => {{
        SessionInfo {
            sender: $sender.clone(),
            receiver: $receiver.clone(),
            sync_odoo: &mut $sync_odoo.lock().unwrap(),
            delayed_process_sender: Some($delayed_sender.clone()),
            noqas_stack: vec![],
            current_noqa: NoqaInfo::None,
        }
    }};
}