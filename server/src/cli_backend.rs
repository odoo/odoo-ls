use lsp_server::Message;
use lsp_types::notification::{LogMessage, Notification, PublishDiagnostics};
use lsp_types::{LogMessageParams, PublishDiagnosticsParams, Uri};
use tracing::{error, info};

use crate::S;
use crate::args::Cli;
use crate::core::config::{ConfigEntry, get_configuration};
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::utils::{PathSanitizer, is_addon_path, is_odoo_path, is_python_path};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

    fn canonicalize_and_validate(
        path: &str,
        is_valid_str: Option<fn(&String) -> bool>,
        is_valid_pb: Option<fn(&Path) -> bool>,
        error_msg: &str,
    ) -> Option<String> {
        match fs::canonicalize(path) {
            Ok(pb) => {
                let sanitized = pb.sanitize();
                let valid = match (is_valid_str, is_valid_pb) {
                    (Some(is_valid_str), Some(is_valid_pb)) => {
                        is_valid_str(&sanitized) && is_valid_pb(&pb)
                    }
                    (Some(is_valid_str), None) => is_valid_str(&sanitized),
                    (None, Some(is_valid_pb)) => is_valid_pb(&pb),
                    (None, None) => true,
                };
                if valid {
                    Some(sanitized)
                } else {
                    error!("{}: {:?}", error_msg, pb);
                    None
                }
            }
            Err(e) => {
                error!("Unable to resolve path: {}. Error: {}", path, e);
                None
            }
        }
    }

/// Basic backend that is used for a single parse execution
pub struct CliBackend {
    cli: Cli,
}

impl CliBackend {

    pub fn new(cli: Cli) -> Self {
        CliBackend {
            cli,
        }
    }

    fn read_config_file(&self, ws_folders: &HashMap<String, String>) -> Option<ConfigEntry> {
        match &self.cli.config_path {
            Some(_) => {
                let config = match get_configuration(ws_folders, &self.cli.config_path) {
                    Ok((config, _)) => config,
                    Err(e) => {
                        error!("Unable to load config file: ({}). Exiting", e);
                        return None;
                    }
                };
                let Some(selected_config) = self.cli.selected_config.clone() else {
                    error!(
                        "No config profile selected. Please provide a config profile using --selected-config when using --config-path arg. Exiting."
                    );
                    return None;
                };
                match config.get(&selected_config) {
                    Some(config) => Some(config.clone()),
                    None => {
                        error!(
                            "Selected config profile ({}) not found in config file. Exiting",
                            selected_config
                        );
                        return None;
                    }
                }
            }
            None => Some(ConfigEntry::default()),
        }
    }

    fn reconcile_args_and_config_file(&self, config: &mut ConfigEntry){
        config.no_typeshed_stubs |= self.cli.no_typeshed_stubs;
        config
            .addons_paths
            .extend(self.cli.addons.iter().flatten().filter_map(|p| {
                canonicalize_and_validate(
                    &p,
                    Some(is_addon_path),
                    None,
                    "Provided addons path is not a valid addon path",
                )
            }));

        if let Some(community_path) = self.cli.community_path.clone() {
            if let Some(pb) = canonicalize_and_validate(
                &community_path,
                Some(is_odoo_path),
                None,
                "Provided community path is not a valid Odoo path",
            ) {
                config.odoo_path = Some(pb);
            }
        }

        if let Some(stubs) = self.cli.stubs.clone() {
            config.additional_stubs.extend(stubs.iter().filter_map(|s| {
                canonicalize_and_validate(
                    s,
                    None,
                    Some(Path::is_dir),
                    "Provided stub path is not a valid directory",
                )
            }));
        }

        if let Some(stdlib) = self.cli.stdlib.clone() {
            if let Some(pb) = canonicalize_and_validate(
                &stdlib,
                None,
                Some(Path::is_dir),
                "Provided stdlib stubs path is not a valid directory",
            ) {
                config.stdlib = pb;
            }
        }

        if let Some(python_path) = self.cli.python.clone() {
            if let Some(path) = canonicalize_and_validate(
                &python_path,
                Some(is_python_path),
                None,
                "Provided python path is not a valid Python executable",
            ) {
                config.python_path = path;
            }
        }
    }

    pub fn run(self) {
        let mut server = SyncOdoo::new();
        let (s, r) = crossbeam_channel::unbounded();
        let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), &mut server);
        session.sync_odoo.load_odoo_addons = false;

        let workspace_folders = self.cli.tracked_folders.clone().unwrap_or(vec![]);
        info!("Using tracked folders: {:?}", workspace_folders);

        for (id, tracked_folder) in workspace_folders.into_iter().enumerate() {
            if let Some(tf) = canonicalize_and_validate(
                &tracked_folder,
                Some(|_| true),
                None,
                "Unable to resolve tracked folder",
            ) {
                let uri = match tf
                    .map(|p| p.sanitize())
                    .and_then(|tf| Uri::from_str(&tf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))
                {
                    Ok(uri) => uri,
                    Err(e) => {
                        error!("Unable to resolve tracked folder: {}, error: {}", tracked_folder, e);
                        continue;
                    }
                };
                session
                    .sync_odoo
                    .get_file_mgr()
                    .borrow_mut()
                    .add_workspace_folder(format!("{}", id), uri);
            }
        }
        let mut config = match self.read_config_file(&session.sync_odoo.get_file_mgr().borrow().get_workspace_folders()) {
            Some(c) => c,
            // Invalid config argument, exit
            None => return,
        };
        self.reconcile_args_and_config_file(&mut config);
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
