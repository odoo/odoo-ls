use lsp_server::Message;
use lsp_types::notification::{LogMessage, Notification, PublishDiagnostics};
use lsp_types::{LogMessageParams, PublishDiagnosticsParams};
use tracing::{error, info};

use crate::core::config::ConfigEntry;
use crate::threads::SessionInfo;
use crate::utils::{get_python_command, PathSanitizer};
use crate::args::Cli;
use std::io::Write;
use std::path::PathBuf;
use std::fs::{self, File};
use serde_json::json;
use crate::core::{config::{DiagMissingImportsMode}, odoo::SyncOdoo};
use crate::S;


/// Basic backend that is used for a single parse execution
pub struct CliBackend {
    cli: Cli
}

impl CliBackend {

    pub fn new(cli: Cli) -> Self {
        CliBackend {
            cli
        }
    }

    pub fn run(&self) {
        let community_path = self.cli.community_path.clone();
        let mut server = SyncOdoo::new();
        let (s, r) = crossbeam_channel::unbounded();
        let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), &mut server);
        session.sync_odoo.load_odoo_addons = false;

        let addons_paths = self.cli.addons.clone().unwrap_or(vec![]);
        info!("Using addons path: {:?}", addons_paths);

        let workspace_folders = self.cli.tracked_folders.clone().unwrap_or(vec![]);
        info!("Using tracked folders: {:?}", workspace_folders);

        for (id, tracked_folder) in workspace_folders.into_iter().enumerate() {
            let tf = fs::canonicalize(tracked_folder.clone());
            if let Ok(tf) = tf {
                let tf = tf.sanitize();
                session.sync_odoo.get_file_mgr().borrow_mut().add_workspace_folder(format!("{}", id), tf);
            } else {
                error!("Unable to resolve tracked folder: {}", tracked_folder);
            }

        }

        let mut config = ConfigEntry::new();
        config.addons_paths = addons_paths.into_iter().map(|p| fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(S!(""))).sanitize()).filter(|x| !x.is_empty()).collect();
        config.odoo_path = Some(fs::canonicalize(community_path.unwrap_or(S!(""))).unwrap_or_else(|_| PathBuf::from(S!(""))).sanitize());
        config.diag_missing_imports = DiagMissingImportsMode::All;
        config.no_typeshed_stubs = self.cli.no_typeshed_stubs;
        config.additional_stubs = self.cli.stubs.clone().unwrap_or(vec![]).into_iter().map(|p| fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(S!(""))).sanitize()).collect();
        config.stdlib = self.cli.stdlib.clone().map(|p| fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(S!(""))).sanitize()).unwrap_or(S!(""));
        config.python_path = self.cli.python.clone().unwrap_or(get_python_command().unwrap_or(S!("")));
        SyncOdoo::init(&mut session, config);

        let output_path = self.cli.output.clone().unwrap_or(S!("output.json"));
        let file = File::create(output_path.clone());
        let mut events = vec![];
        if let Ok(mut file) = file {
            while !r.is_empty() {
                let msg = r.recv();
                if let Ok(msg) = msg {
                    match msg {
                        Message::Notification(n) => {
                            match n.method.as_str() {
                                LogMessage::METHOD => {
                                    let params: LogMessageParams = serde_json::from_value(n.params).unwrap();
                                    events.push(json!({
                                        "type": "log",
                                        "severity": params.typ,
                                        "message": params.message
                                    }))
                                },
                                PublishDiagnostics::METHOD => {
                                    let mut diagnostics = vec![];
                                    let params: PublishDiagnosticsParams = serde_json::from_value(n.params).unwrap();
                                    for diagnostic in params.diagnostics.iter() {
                                        diagnostics.push(serde_json::to_value(diagnostic).unwrap());
                                    }
                                    events.push(json!({
                                        "type": "diagnostic",
                                        "uri": params.uri,
                                        "version": params.version,
                                        "diagnostics": diagnostics
                                    }));
                                },
                                _ => {error!("not handled method: {}", n.method)}
                            }
                        },
                        Message::Request(_) => {
                            error!("No request should be sent to client as we are in cli mode.");
                        },
                        Message::Response(_) => {
                            error!("No response should be sent to client as we are in cli mode.");
                        }
                    }
                } else {
                    error!("Unable to recv a message");
                }
            }
            let json_string = json!({"events": events});
            if let Err(e) = file.write_all(serde_json::to_string_pretty(&json_string).unwrap().as_bytes()) {
                error!("Unable to write to {}: {}", output_path, e)
            }
        }
    }
}
