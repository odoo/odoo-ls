use std::{borrow::Borrow, sync::{Arc, Mutex}, time::Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use lsp_server::{Message, RequestId, Response, ResponseError};
use lsp_types::{notification::{DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders,
    DidCloseTextDocument, DidCreateFiles, DidDeleteFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, LogMessage,
    Notification}, request::{Completion, GotoDefinition, GotoTypeDefinitionResponse, HoverRequest, Request, Shutdown},
    CompletionResponse, Hover, LogMessageParams, MessageType};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing::{error, warn};

use crate::{core::{config::RefreshMode, odoo::{Odoo, SyncOdoo}}, server::ServerError, S};

pub struct SessionInfo<'a> {
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    pub sync_odoo: &'a mut SyncOdoo,
    delayed_process_sender: Option<Sender<DelayedProcessingMessage>> //option, because not available in read thread (by convention, no real need to not provide it)
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
            Ok(msg) => return Err(ServerError::ServerError("Not an answer.".to_string())),
            Err(RecvError) => return Err(ServerError::ServerError("Server disconnected".to_string())),
        }
    }

    pub fn request_process(session: &mut SessionInfo) {
        if let Some(sender) = &session.delayed_process_sender {
            if !session.sync_odoo.need_rebuild && session.sync_odoo.config.refresh_mode == RefreshMode::Adaptive && session.sync_odoo.get_rebuild_queue_size() < 10 {
                SyncOdoo::process_rebuilds(session);
            } else {
                let _ = sender.send(DelayedProcessingMessage::PROCESS(std::time::Instant::now()));
            }
        } else {
            SyncOdoo::process_rebuilds(session);
        }
    }

    pub fn request_reload(session: &mut SessionInfo) {
        if let Some(sender) = &session.delayed_process_sender {
            let _ = sender.send(DelayedProcessingMessage::REBUILD(std::time::Instant::now()));
        } else {
            SyncOdoo::reset(session, session.sync_odoo.config.clone());
        }
    }

    pub fn update_auto_refresh_delay(&self, delay: u64) {
        if let Some(sender) = &self.delayed_process_sender {
            let _ = sender.send(DelayedProcessingMessage::UPDATE_DELAY(delay));
        }
    }

    /* use it for test or tools, that do not need to connect to the server, and only want a fake session to use SyncOdoo */
    pub fn new_from_custom_channel(sender: Sender<Message>, receiver: Receiver<Message>, sync_odoo: &'a mut SyncOdoo, delayed_process_sender: Option<Sender<DelayedProcessingMessage>>) -> Self {
        Self {
            sender,
            receiver,
            sync_odoo,
            delayed_process_sender: delayed_process_sender
        }
    }
}

fn to_value<T: Serialize + std::fmt::Debug>(result: Result<Option<T>, ResponseError>) -> (Option<Value>, Option<ResponseError>) {
    let value = match &result {
        Ok(Some(r)) => Some(serde_json::json!(r)),
        Ok(None) => Some(serde_json::Value::Null),
        Err(e) => None
    };
    let mut error = None;
    if result.is_err() {
        error = Some(result.unwrap_err());
    }
    (value, error)
}

pub enum DelayedProcessingMessage {
    UPDATE_DELAY(u64),
    PROCESS(Instant),
    REBUILD(Instant),
    EXIT,
}

pub fn delayed_changes_process_thread(sender_session: Sender<Message>, receiver_session: Receiver<Message>, receiver: Receiver<DelayedProcessingMessage>, sync_odoo: Arc<Mutex<SyncOdoo>>) {
    const MAX_DELAY: u64 = 15000;
    let mut normal_delay = std::time::Duration::from_millis(std::cmp::min(sync_odoo.lock().unwrap().config.auto_save_delay, MAX_DELAY));
    loop {
        let mut rebuild = false;
        let mut delay = normal_delay;
        let msg = receiver.recv();
        match msg {
            Ok(DelayedProcessingMessage::EXIT) => {
                return;
            },
            Ok(DelayedProcessingMessage::UPDATE_DELAY(duration)) => {
                normal_delay = std::time::Duration::from_millis(std::cmp::min(duration, MAX_DELAY));
            }
            Ok(DelayedProcessingMessage::REBUILD(time) | DelayedProcessingMessage::PROCESS(time)) => {
                if matches!(msg, Ok(DelayedProcessingMessage::REBUILD(_))) {
                    rebuild = true;
                }
                let mut last_time = time;
                let mut to_wait = (time + delay) - std::time::Instant::now();
                while to_wait.as_millis() > 0 {
                    std::thread::sleep(to_wait);
                    to_wait = std::time::Duration::ZERO;
                    loop {
                        let new_msg = receiver.try_recv();
                        match new_msg {
                            Ok(DelayedProcessingMessage::EXIT) => {return;},
                            Ok(DelayedProcessingMessage::UPDATE_DELAY(duration)) => {
                                delay = std::time::Duration::from_millis(std::cmp::min(duration, MAX_DELAY));
                            }
                            Ok(DelayedProcessingMessage::PROCESS(t)) => {
                                if t > last_time {
                                    to_wait = (t + delay) - std::time::Instant::now();
                                    last_time = t;
                                }
                            },
                            Ok(DelayedProcessingMessage::REBUILD(t)) => {
                                rebuild = true;
                                delay = std::time::Duration::from_millis(std::cmp::max(normal_delay.as_millis() as u64, 4000));
                                if t > last_time {
                                    to_wait = (t + delay) - std::time::Instant::now();
                                    last_time = t;
                                }
                            }
                            Err(TryRecvError::Empty) => {
                                break;
                            }
                            Err(e) => {return;}
                        }
                    }
                }
                {
                    let mut session = SessionInfo{
                        sender: sender_session.clone(),
                        receiver: receiver_session.clone(),
                        sync_odoo: &mut sync_odoo.lock().unwrap(),
                        delayed_process_sender: None
                    };
                    if rebuild {
                        let config = session.sync_odoo.config.clone();
                        SyncOdoo::reset(&mut session, config);
                    } else {
                        SyncOdoo::process_rebuilds(&mut session);
                    }
                }
            }
            Err(e) => {
                return;
            }
        }
    }
}

pub fn message_processor_thread_main(sync_odoo: Arc<Mutex<SyncOdoo>>, generic_receiver: Receiver<Message>, sender: Sender<Message>, receiver: Receiver<Message>, delayed_process_sender: Sender<DelayedProcessingMessage>) {
    loop {
        let msg = generic_receiver.recv();
        if let Err(e) = msg {
            error!("Got an RecvError, exiting thread");
            break;
        }
        let msg = msg.unwrap();
        let mut session = SessionInfo{
            sender: sender.clone(),
            receiver: receiver.clone(),
            sync_odoo: &mut sync_odoo.lock().unwrap(),
            delayed_process_sender: Some(delayed_process_sender.clone())
        };
        match msg {
            Message::Request(r) => {
                let (value, error) = match r.method.as_str() {
                    Completion::METHOD => {
                        //Handle completion in main because updates has to be done before the autocompletion
                        //Moreover, autocompletion will trigger a process_rebuild if needed.
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
                    DidOpenTextDocument::METHOD => { Odoo::handle_did_open(&mut session, serde_json::from_value(n.params).unwrap()); }
                    DidChangeConfiguration::METHOD => { Odoo::handle_did_change_configuration(&mut session, serde_json::from_value(n.params).unwrap()) }
                    DidChangeWorkspaceFolders::METHOD => { Odoo::handle_did_change_workspace_folders(&mut session, serde_json::from_value(n.params).unwrap()) }
                    DidChangeTextDocument::METHOD => { Odoo::handle_did_change(&mut session, serde_json::from_value(n.params).unwrap()); }
                    DidCloseTextDocument::METHOD => {}
                    DidSaveTextDocument::METHOD => { Odoo::handle_did_save(&mut session, serde_json::from_value(n.params).unwrap()); }
                    DidRenameFiles::METHOD => { Odoo::handle_did_rename(&mut session, serde_json::from_value(n.params).unwrap()); }
                    DidCreateFiles::METHOD => { Odoo::handle_did_create(&mut session, serde_json::from_value(n.params).unwrap()); }
                    DidDeleteFiles::METHOD => { Odoo::handle_did_delete(&mut session, serde_json::from_value(n.params).unwrap()); }
                    DidChangeWatchedFiles::METHOD => { Odoo::handle_did_change_watched_files(&mut session, serde_json::from_value(n.params).unwrap())}
                    "custom/server/register_capabilities" => { Odoo::register_capabilities(&mut session); }
                    "custom/server/init" => { Odoo::init(&mut session); }
                    Shutdown::METHOD => { warn!("Main thread - got shutdown."); break;}
                    _ => {error!("Notification not handled by main thread: {}", n.method)}
                }
            },
            Message::Response(r) => {
                error!("Error: Responses should not arrives in generic channel. Exiting thread");
                break;
            }
        }
    }
}

pub fn message_processor_thread_read(sync_odoo: Arc<Mutex<SyncOdoo>>, generic_receiver: Receiver<Message>, sender: Sender<Message>, receiver: Receiver<Message>) {
    loop {
        let msg = generic_receiver.recv();
        if let Err(e) = msg {
            error!("Got an RecvError, exiting thread");
            break;
        }
        let msg = msg.unwrap();
        let mut session = SessionInfo{
            sender: sender.clone(),
            receiver: receiver.clone(),
            sync_odoo: &mut sync_odoo.lock().unwrap(), //TODO work on read access
            delayed_process_sender: None
        };
        match msg {
            Message::Request(r) => {
                let (value, error) = match r.method.as_str() {
                    HoverRequest::METHOD => {
                        to_value::<Hover>(Odoo::handle_hover(&mut session, serde_json::from_value(r.params).unwrap()))
                    },
                    GotoDefinition::METHOD => {
                        to_value::<GotoTypeDefinitionResponse>(Odoo::handle_goto_definition(&mut session, serde_json::from_value(r.params).unwrap()))
                    },
                    _ => {error!("Request not handled by read thread: {}", r.method); (None, Some(ResponseError{
                        code: 1,
                        message: S!("Request not handled by the server"),
                        data: None
                    }))}
                };
                sender.send(Message::Response(Response { id: r.id, result: value, error: error })).unwrap();
            },
            Message::Notification(r) => {
                match r.method.as_str() {
                    Shutdown::METHOD => { warn!("Read thread - got shutdown."); break;}
                    _ => {error!("Notification not handled by read thread: {}", r.method)}
                }
            },
            Message::Response(r) => {
                error!("Error: Responses should not arrives in generic channel. Exiting thread");
                break;
            }
        }
    }
}

// pub fn message_processor_thread_reactive(sender: Sender<Message>, receiver: Receiver<Message>) {
//     loop {
//         let msg = receiver.recv();
//         match msg {
//             Ok(msg) => {
//                 println!("Not handled for now");
//             },
//             Err(e) => {
//                 break;
//             }
//         }
//     }
// }