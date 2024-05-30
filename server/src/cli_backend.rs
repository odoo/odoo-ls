use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::{args::Cli, core::messages::Msg};
use std::io::Write;
use std::{path::PathBuf};
use std::cell::RefCell;
use std::fs::File;
use serde_json::json;
use crate::{backend::Backend, core::{config::{Config, DiagMissingImportsMode}, messages::SyncChannel, odoo::SyncOdoo}};
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
        let sync_channel = SyncChannel { messages: RefCell::new(Vec::new()) };
        let msg_handler = MsgHandler::SYNC_CHANNEL(sync_channel);
        let mut server = SyncOdoo::new(msg_handler);
        server.load_odoo_addons = false;

        let addons_paths = self.cli.addons.clone().unwrap_or(vec![]);
        println!("Using addons path: {:?}", addons_paths);

        let workspace_folders = self.cli.tracked_folders.clone().unwrap_or(vec![]);
        println!("Using tracked folders: {:?}", workspace_folders);

        for tracked_folder in workspace_folders {
            server.get_file_mgr().borrow_mut().add_workspace_folder(tracked_folder);
        }

        server.init(addons_paths,
        community_path,
        S!("python3"),
        crate::core::config::RefreshMode::Off,
        10000,
        DiagMissingImportsMode::All);

        let output_path = self.cli.output.clone().unwrap_or(S!("output.json"));
        let file = File::create(output_path.clone());
        let mut events = vec![];
        if let Ok(mut file) = file {
            if let MsgHandler::SYNC_CHANNEL(channel) = server.msg_sender {
                for (index, msg) in channel.messages.borrow().iter().enumerate() {
                    match msg {
                        Msg::MPSC_SHUTDOWN() => {
                            events.push(json!({
                                "type": "log",
                                "severity": "info",
                                "message": "End of execution"
                            }))
                        },
                        Msg::LOG_INFO(l) => {
                            events.push(json!({
                                "type": "log",
                                "severity": "info",
                                "message": l.as_str()
                            }))
                        },
                        Msg::LOG_WARNING(l) => {
                            events.push(json!({
                                "type": "log",
                                "severity": "warning",
                                "message": l.as_str()
                            }))
                        },
                        Msg::LOG_ERROR(l) => {
                            events.push(json!({
                                "type": "log",
                                "severity": "error",
                                "message": l.as_str()
                            }))
                        },
                        Msg::DIAGNOSTIC(d) => {
                            let mut diagnostics = vec![];
                            for diag in d.diags.iter() {
                                diagnostics.push(serde_json::to_value(diag).unwrap());
                            }
                            events.push(json!({
                                "type": "diagnostic",
                                "uri": d.uri,
                                "version": d.version,
                                "diagnostics": diagnostics
                            }));
                        }
                    }
                }
                let json_string = json!({"events": events});
                if let Err(e) = file.write_all(serde_json::to_string_pretty(&json_string).unwrap().as_bytes()) {
                    println!("Unable to write to {}: {}", output_path, e)
                }
            }
        }
    }
}
