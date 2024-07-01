use lsp_server::Message;
use lsp_types::notification::{LogMessage, Notification, PublishDiagnostics};
use lsp_types::{DiagnosticSeverity, LogMessageParams, PublishDiagnosticsParams};

use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;
use crate::{args::Cli, core::messages::Msg};
use std::io::Write;
use std::{path::PathBuf};
use std::cell::RefCell;
use std::fs::File;
use serde_json::json;
use crate::{core::{config::{Config, DiagMissingImportsMode}, messages::SyncChannel, odoo::SyncOdoo}};
use crate::core::messages::MsgHandler;
use crate::S;
use crate::core::file_mgr::FileMgr;


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
        let community_path = self.cli.community_path.clone().expect("Please provide a Community path");
        let mut server = SyncOdoo::new();
        let (s, r) = crossbeam_channel::unbounded();
        let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), &mut server);
        session.sync_odoo.load_odoo_addons = false;

        let addons_paths = self.cli.addons.clone().unwrap_or(vec![]);
        println!("Using addons path: {:?}", addons_paths);

        let workspace_folders = self.cli.tracked_folders.clone().unwrap_or(vec![]);
        println!("Using tracked folders: {:?}", workspace_folders);

        for tracked_folder in workspace_folders {
            session.sync_odoo.get_file_mgr().borrow_mut().add_workspace_folder(PathBuf::from(tracked_folder).sanitize());
        }

        let mut config = Config::new();
        config.addons = addons_paths;
        config.odoo_path = community_path;
        config.python_path = S!("python3");
        config.refresh_mode = crate::core::config::RefreshMode::Off;
        config.diag_missing_imports = DiagMissingImportsMode::All;
        config.no_typeshed = self.cli.no_typeshed;
        config.additional_stubs = self.cli.stubs.clone().unwrap_or(vec![]);
        config.stdlib = self.cli.stdlib.clone().unwrap_or(S!(""));
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
                                _ => {println!("not handled method: {}", n.method)}
                            }
                        },
                        Message::Request(r) => {
                            println!("No request should be sent to client as we are in cli mode.");
                        },
                        Message::Response(r) => {
                            println!("No response should be sent to client as we are in cli mode.");
                        }
                    }
                } else {
                    println!("Unable to recv a message");
                }
            }
            let json_string = json!({"events": events});
            if let Err(e) = file.write_all(serde_json::to_string_pretty(&json_string).unwrap().as_bytes()) {
                println!("Unable to write to {}: {}", output_path, e)
            }
        }
    }
}
