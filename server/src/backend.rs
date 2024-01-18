use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use serde_json::to_value;
use crate::odoo::Odoo;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub odoo: Option<Odoo>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
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
        self.init();
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl Backend {
    pub async fn client_config_changed(&self) {
        
    }

    pub async fn client_ready(&self) {
        self.client.log_message(MessageType::INFO, format!("Client ready !")).await
    }

    async fn init(&mut self) {
        self.client.log_message(MessageType::INFO, "Building new Odoo knowledge database").await;
        self.odoo = Some(Odoo::new());
    }
}