use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use serde_json::to_value;
use crate::core::odoo::{Odoo, Msg};
use serde;
use tokio::sync::Mutex;
use std::sync::Arc;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub odoo: Arc<Mutex<Odoo>>,
    pub msg_receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<Msg>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Odoo Language Server".to_string(),
                version: Some("0.2.0".to_string())
            }),
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![
                FileSystemWatcher {
                    glob_pattern: GlobPattern::String("**".to_string()),
                    kind: Some(WatchKind::Change),
                },
            ],
        };
        match self.client.register_capability(vec![
            Registration {
                id: "workspace/didChangeWatchedFiles".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: Some(to_value(options).unwrap()),
            },
            Registration {
                id: "workspace/didChangeConfiguration".to_string(),
                method: "workspace/didChangeConfiguration".to_string(),
                register_options: None,
            },
        ]).await {
            Ok(_) => (),
            Err(e) => self.client.log_message(MessageType::ERROR, format!("Error registering capabilities: {:?}", e)).await,
        }
        self.client.log_message(MessageType::INFO, "server initialized!").await;
        let msg_receiver = self.msg_receiver.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let mut msg_receiver = msg_receiver.lock().await;
            handle_msgs(&mut msg_receiver, &client).await;
        });
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(Some(Hover {
            contents: HoverContents::Scalar(
                MarkedString::String("You're hovering!".to_string())
            ),
            range: None
        }))
    }

    async fn shutdown(&self) -> Result<()> {
        let odoo = self.odoo.lock().await;
        odoo.msg_sender.send(Msg::MPSC_SHUTDOWN()).await.unwrap();
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ReadyParams {
    value1: u32,
}

impl Backend {
    pub async fn client_config_changed(&self) {
        
    }

    pub async fn client_ready(&self, params: ReadyParams) {
        self.client.log_message(MessageType::INFO, format!("Client ready !")).await;
        let mut odoo = self.odoo.lock().await;
        odoo.init(&self.client).await;
    }
}

pub async fn handle_msgs(msg_receiver: &mut tokio::sync::mpsc::Receiver<Msg>, client: &Client) {
    while let Some(msg) = msg_receiver.recv().await {
        if !handle_msg(msg, client).await {
            msg_receiver.close()
        }
    }
}

pub async fn handle_msg(msg: Msg, client: &Client) -> bool {
    match msg {
        Msg::LOG_INFO(msg) => {
            client.log_message(MessageType::INFO, msg).await;
        },
        Msg::LOG_WARNING(msg) => {
            client.log_message(MessageType::WARNING, msg).await;
        },
        Msg::LOG_ERROR(msg) => {
            client.log_message(MessageType::ERROR, msg).await;
        },
        Msg::DIAGNOSTIC(msg) => {
            client.log_message(MessageType::INFO, msg).await;
        },
        Msg::MPSC_SHUTDOWN() => {
            println!("shutdown mpsc channel");
            return false;
        }
    }
    return true;
}