use std::borrow::BorrowMut;
use std::cell::RefCell;

use tower_lsp::lsp_types::Diagnostic;
use url::Url;

#[derive(Debug)]
pub enum Msg {
    LOG_INFO(String), //send a log INFO to client
    LOG_WARNING(String), //send a log WARNING to client
    LOG_ERROR(String), //send a log ERROR to client
    DIAGNOSTIC(MsgDiagnostic), //send a diagnostic to client
    MPSC_SHUTDOWN(), //Shutdown the mpsc channel. No message can be sent afterthat
}

#[derive(Debug)]
pub struct MsgDiagnostic {
    pub uri: Url,
    pub diags: Vec<Diagnostic>,
    pub version: Option<i32>,
}

#[derive(Debug)]
pub struct SyncChannel {
    pub messages: RefCell<Vec<Msg>>
}

#[derive(Debug)]
pub enum MsgHandler {
    TOKIO_MPSC(tokio::sync::mpsc::Sender<Msg>),
    SYNC_CHANNEL(SyncChannel)
}

impl MsgHandler {
    pub fn send(&self, msg: Msg) {
        match self {
            MsgHandler::TOKIO_MPSC(sender) => {
                sender.blocking_send(msg).expect("error sending message");
            },
            MsgHandler::SYNC_CHANNEL(channel) => {
                channel.messages.borrow_mut().push(msg);
            }
        }
    }
}