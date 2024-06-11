use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use serde_json::to_value;
use crate::core::file_mgr::FileMgr;
use crate::S;
use crate::core::config::RefreshMode;
use crate::core::odoo::Odoo;
use crate::core::messages::Msg;
use crate::features::completion::CompletionFeature;
use crate::features::definition::DefinitionFeature;
use crate::features::hover::HoverFeature;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio::time::timeout;
use std::sync::Arc;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub odoo: Arc<Mutex<Odoo>>,
    pub msg_receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<Msg>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(workspace_folders) = params.workspace_folders {
            let odoo = self.odoo.lock().await;
            let mut sync_odoo = odoo.odoo.lock().unwrap();
            let file_mgr = sync_odoo.get_file_mgr();
            let mut file_mgr = file_mgr.borrow_mut();
            for added in workspace_folders.iter() {
                let path = added.uri.to_file_path().expect("unable to get normalized file path").as_os_str().to_str().unwrap().to_string();
                file_mgr.add_workspace_folder(path);
            }
        }
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Odoo Language Server".to_string(),
                version: Some("0.2.0".to_string())
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    change: Some(TextDocumentSyncKind::FULL),
                    ..TextDocumentSyncOptions::default()
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
                    ..WorkspaceServerCapabilities::default()
                }),
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
        let textDocumentChangeRegistrationOptions = TextDocumentChangeRegistrationOptions {
            document_selector: None,
            sync_kind: 1, //TextDocumentSyncKind::FULL
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
            Registration {
                id: "textDocument/didOpen".to_string(),
                method: "textDocument/didOpen".to_string(),
                register_options: None,
            },
            Registration {
                id: "textDocument/didChange".to_string(),
                method: "textDocument/didChange".to_string(),
                register_options: Some(to_value(textDocumentChangeRegistrationOptions).unwrap()),
            },
            Registration {
                id: "textDocument/didClose".to_string(),
                method: "textDocument/didClose".to_string(),
                register_options: None,
            }
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

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.client.log_message(MessageType::INFO, format!("Hover requested on {} at {} - {}",
            params.text_document_position_params.text_document.uri.to_string(),
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character)).await;
        let mut odoo = timeout(Duration::from_millis(1000), self.odoo.lock()).await;
        if let Err(_) = odoo {
            return Ok(Some(Hover {
                contents: HoverContents::Scalar(
                    MarkedString::String("Odoo is still loading. Please wait...".to_string())
                ),
                range: None
            }))
        }
        let path = FileMgr::uri2pathname(params.text_document_position_params.text_document.uri.as_str());
        let mut odoo = odoo.unwrap();
        {
            let mut sync_odoo = odoo.odoo.lock().unwrap();
            if params.text_document_position_params.text_document.uri.to_string().ends_with(".py") {
                if let Some(file_symbol) = sync_odoo.get_file_symbol(&PathBuf::from(path.clone())) {
                    let file_info = sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                    if let Some(file_info) = file_info {
                        if file_info.borrow().ast.is_some() {
                            return HoverFeature::get_hover(&mut sync_odoo, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.client.log_message(MessageType::INFO, format!("GoToDefinition requested on {} at {} - {}",
            params.text_document_position_params.text_document.uri.to_string(),
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character)).await;
        let mut odoo = timeout(Duration::from_millis(1000), self.odoo.lock()).await;
        if let Err(_) = odoo {
            return Ok(None);
        }
        let path = FileMgr::uri2pathname(params.text_document_position_params.text_document.uri.as_str());
        let mut odoo = odoo.unwrap();
        {
            let mut sync_odoo = odoo.odoo.lock().unwrap();
            if params.text_document_position_params.text_document.uri.to_string().ends_with(".py") {
                if let Some(file_symbol) = sync_odoo.get_file_symbol(&PathBuf::from(path.clone())) {
                    let file_info = sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                    if let Some(file_info) = file_info {
                        if file_info.borrow().ast.is_some() {
                            return DefinitionFeature::get_location(&mut sync_odoo, &file_symbol, &file_info, params.text_document_position_params.position.line, params.text_document_position_params.position.character);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.client.log_message(MessageType::INFO, format!("Completion requested at {}:{}-{}",
            params.text_document_position.text_document.uri,
            params.text_document_position.position.line,
            params.text_document_position.position.character
            )).await;
        let mut odoo = timeout(Duration::from_millis(1000), self.odoo.lock()).await;
        if let Err(_) = odoo {
            return Ok(None);
        }
        let path = FileMgr::uri2pathname(params.text_document_position.text_document.uri.as_str());
        let mut odoo = odoo.unwrap();
        {
            let mut sync_odoo = odoo.odoo.lock().unwrap();
            if params.text_document_position.text_document.uri.to_string().ends_with(".py") {
                if let Some(file_symbol) = sync_odoo.get_file_symbol(&PathBuf::from(path.clone())) {
                    let file_info = sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path);
                    if let Some(file_info) = file_info {
                        if file_info.borrow().ast.is_some() {
                            return CompletionFeature::autocomplete(&mut sync_odoo, &file_symbol, &file_info, params.text_document_position.position.line, params.text_document_position.position.character);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut odoo = self.odoo.lock().await;
        let mut delay = 1000;
        {
            let sync_odoo = odoo.odoo.lock().unwrap();
            if sync_odoo.config.refresh_mode != RefreshMode::AfterDelay {
                return
            }
            delay = sync_odoo.config.auto_save_delay;
        }
        tokio::time::sleep(Duration::from_millis(delay)).await;
        let path = params.text_document.uri.to_file_path().unwrap();
        let version = params.text_document.version;
        //TODO get source by keeping diff?
        let source = params.content_changes[0].text.clone();
        odoo.reload_file(&self.client, path, source, version).await;
    }

    async fn shutdown(&self) -> Result<()> {
        let odoo = self.odoo.lock().await;
        odoo.msg_sender.send(Msg::MPSC_SHUTDOWN()).await.unwrap();
        Ok(())
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let odoo = self.odoo.lock().await;
        let mut sync_odoo = odoo.odoo.lock().unwrap();
        let file_mgr = sync_odoo.get_file_mgr();
        let mut file_mgr = file_mgr.borrow_mut();
        for added in params.event.added {
            file_mgr.add_workspace_folder(added.uri.to_string());
        }
        for removed in params.event.removed {
            file_mgr.remove_workspace_folder(removed.uri.to_string());
        }
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
            client.publish_diagnostics(msg.uri, msg.diags, msg.version).await;
        },
        Msg::MPSC_SHUTDOWN() => {
            println!("shutdown mpsc channel");
            return false;
        }
    }
    return true;
}