use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::{args::Cli, core::messages::Msg};
use std::{borrow::Borrow, path::PathBuf};
use std::cell::RefCell;
use crate::{backend::Backend, core::{config::{Config, DiagMissingImportsMode}, messages::SyncChannel, odoo::SyncOdoo}};
use crate::core::messages::MsgHandler;
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
        let community_path = self.cli.community_path.clone().expect("Please provide a Community path");
        let sync_channel = SyncChannel { messages: RefCell::new(Vec::new()) };
        let msg_handler = MsgHandler::SYNC_CHANNEL(sync_channel);
        let mut server = SyncOdoo::new(msg_handler);
        server.load_odoo_addons = false;

        let addons_paths = self.cli.addons.clone().unwrap_or(vec![]);
        println!("Using addons path: {:?}", addons_paths);

        let workspace_folders = self.cli.tracked_folders.clone().unwrap_or(vec![]);
        println!("Using tracked folders: {:?}", workspace_folders);

        server.init(addons_paths,
        community_path,
        S!("python3"),
        crate::core::config::RefreshMode::Off,
        10000,
        DiagMissingImportsMode::All);

        println!("\n\nOUTPUT\n\n");

        if let MsgHandler::SYNC_CHANNEL(channel) = server.msg_sender {
            for msg in channel.messages.borrow().iter() {
                match msg {
                    Msg::MPSC_SHUTDOWN() => {
                        println!("End of execution");
                    },
                    Msg::LOG_INFO(l) => {
                        println!("[INFO]: {l}");
                    },
                    Msg::LOG_WARNING(l) => {
                        println!("[WARN]: {l}");
                    },
                    Msg::LOG_ERROR(l) => {
                        println!("[ERROR]: {l}");
                    },
                    Msg::DIAGNOSTIC(d) => {
                        let mut output = String::from("{\n");
                        output += &format!("uri: {}\n", d.uri);
                        output += "diags:\n";
                        for diag in d.diags.iter() {
                            output += "\t{\n";
                            let severity = diag.severity;
                            let mut severity_string = S!("");
                            if let Some(s) = severity {
                                severity_string = format!("[{:?}]", s);
                            }

                            output += &format!("\t{} {}\n", severity_string, diag.message);
                            output += &format!("\trange: {}:{} - {}:{}", diag.range.start.line, diag.range.start.character, diag.range.end.line, diag.range.end.character);
                        }
                        output += "}\n";
                    }
                }
            }
        }
    }
}
