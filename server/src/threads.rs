use std::{collections::HashMap, error::Error, sync::{Arc, Mutex, RwLock, RwLockReadGuard}, thread::JoinHandle};

use crossbeam_channel::{Receiver, Sender};
use lsp_server::{Message, RequestId, Response, ResponseError};
use lsp_types::{notification::{DidChangeConfiguration, DidChangeTextDocument, DidChangeWorkspaceFolders, DidCloseTextDocument, DidCreateFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, LogMessage, Notification}, request::{Completion, GotoDefinition, GotoTypeDefinitionResponse, HoverRequest, Request}, CompletionResponse, Hover, HoverParams, LogMessageParams, MessageType};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::{core::odoo::{Odoo, SyncOdoo}, server::{Server, ServerError}, S};

pub struct SessionInfo<'a> {
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    pub sync_odoo: &'a mut SyncOdoo
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
            println!("Unable to serialize parameters for method {}", method);
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
                if let Some(error) = r.error {
                    println!("Got error for response of {}: {}", method, error.message);
                    return Err(ServerError::ResponseError(error));
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

    /* use it for test or tools, that do not need to connect to the server, and only want a fake session to use SyncOdoo */
    pub fn new_from_custom_channel(sender: Sender<Message>, receiver: Receiver<Message>, sync_odoo: &'a mut SyncOdoo) -> Self {
        Self {
            sender,
            receiver,
            sync_odoo
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

pub fn message_processor_thread_main(sync_odoo: Arc<Mutex<SyncOdoo>>, generic_receiver: Receiver<Message>, sender: Sender<Message>, receiver: Receiver<Message>) {
    loop {
        let msg = generic_receiver.recv();
        if let Err(e) = msg {
            println!("Got an RecvError, exiting thread");
            break;
        }
        let msg = msg.unwrap();
        let mut session = SessionInfo{
            sender: sender.clone(),
            receiver: receiver.clone(),
            sync_odoo: &mut sync_odoo.lock().unwrap()
        };
        match msg {
            Message::Request(r) => {
                let (value, error) = match r.method.as_str() {
                    _ => {println!("Request not handled by read thread: {}", r.method); (None, Some(ResponseError{
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
                    DidRenameFiles::METHOD => {}
                    DidCreateFiles::METHOD => {}
                    "custom/server/register_capabilities" => { Odoo::register_capabilities(&mut session); }
                    "custom/server/init" => { Odoo::init(&mut session); }
                    _ => {println!("Notification not handled by main thread: {}", n.method)}
                }
            },
            Message::Response(r) => {
                println!("Error: Responses should not arrives in generic channel. Exiting thread");
                break;
            }
        }
    }
}

pub fn message_processor_thread_read(sync_odoo: Arc<Mutex<SyncOdoo>>, generic_receiver: Receiver<Message>, sender: Sender<Message>, receiver: Receiver<Message>) {
    loop {
        let msg = generic_receiver.recv();
        if let Err(e) = msg {
            println!("Got an RecvError, exiting thread");
            break;
        }
        let msg = msg.unwrap();
        let mut session = SessionInfo{
            sender: sender.clone(),
            receiver: receiver.clone(),
            sync_odoo: &mut sync_odoo.lock().unwrap() //TODO work on read access
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
                    Completion::METHOD => {
                        to_value::<CompletionResponse>(Odoo::handle_autocomplete(&mut session, serde_json::from_value(r.params).unwrap()))
                    },
                    _ => {println!("Request not handled by read thread: {}", r.method); (None, Some(ResponseError{
                        code: 1,
                        message: S!("Request not handled by the server"),
                        data: None
                    }))}
                };
                sender.send(Message::Response(Response { id: r.id, result: value, error: error })).unwrap();
            },
            Message::Notification(r) => {
                match r.method.as_str() {
                    _ => {println!("Notification not handled by read thread: {}", r.method)}
                }
            },
            Message::Response(r) => {
                println!("Error: Responses should not arrives in generic channel. Exiting thread");
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