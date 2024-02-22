use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use serde_json::to_value;
use crate::core::odoo::Odoo;
use serde;
use tokio::sync::Mutex;
use std::sync::Arc;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub odoo: Arc<Mutex<Odoo>>,
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