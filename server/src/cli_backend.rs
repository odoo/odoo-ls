use lsp_server::Message;
use lsp_types::notification::{LogMessage, Notification, PublishDiagnostics};
use lsp_types::{LogMessageParams, PublishDiagnosticsParams};
use tracing::{error, info};

use crate::S;
use crate::args::Cli;
use crate::core::config::{ConfigEntry, ConfigKey, DEFAULT_PROFILE_NAME, get_configuration};
use crate::core::file_mgr::FileMgr;
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::utils::{PathSanitizer, is_addon_path, is_odoo_path, is_python_path};
use serde_json::json;
use crate::utils::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

fn canonicalize_and_validate(
    path: &str,
    is_valid_str: Option<fn(&str) -> bool>,
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

    fn read_config_file(&self, session: &mut SessionInfo) -> Option<ConfigEntry> {
        info!("CLI selected config file: {:?}", self.cli.config_path);
        session.sync_odoo.config_path = self.cli.config_path.clone();

        let config = match get_configuration(session) {
            Ok((config, _)) => config,
            Err(e) => {
                error!("Unable to load config file: ({}). Exiting", e);
                return None;
            }
        };

        let selected_config = match self.cli.selected_config.clone() {
            Some(selected_config) => {
                info!("CLI selected config profile: {:?}", selected_config);
                selected_config
            },
            None => {
                info!("No CLI selected config profile, using default: {:?}", DEFAULT_PROFILE_NAME);
                DEFAULT_PROFILE_NAME.to_string()
            }
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

    fn reconcile_args_and_config_file(&self, config: &mut ConfigEntry) {
        if self.cli.no_typeshed_stubs {
            config.set_bool(ConfigKey::NoTypeshedStubs, true);
        }
        config.extend_string_list(
            ConfigKey::AddonsPaths,
            self.cli.addons.iter().flatten().filter_map(|p| {
                canonicalize_and_validate(
                    p,
                    Some(is_addon_path),
                    None,
                    "Provided addons path is not a valid addon path",
                )
            }),
        );

        if let Some(community_path) = self.cli.community_path.clone()
            && let Some(pb) = canonicalize_and_validate(
                &community_path,
                Some(is_odoo_path),
                None,
                "Provided community path is not a valid Odoo path",
            ) {
                config.set_str(ConfigKey::OdooPath, pb);
            }

        if let Some(stubs) = self.cli.stubs.clone() {
            config.extend_string_list(
                ConfigKey::AdditionalStubs,
                stubs.iter().filter_map(|s| {
                    canonicalize_and_validate(
                        s,
                        None,
                        Some(Path::is_dir),
                        "Provided stub path is not a valid directory",
                    )
                }),
            );
        }

        if let Some(stdlib) = self.cli.stdlib.clone()
            && let Some(pb) = canonicalize_and_validate(
                &stdlib,
                None,
                Some(Path::is_dir),
                "Provided stdlib stubs path is not a valid directory",
            ) {
                config.set_str(ConfigKey::Stdlib, pb);
            }

        if let Some(python_path) = self.cli.python.clone()
            && let Some(path) = canonicalize_and_validate(
                &python_path,
                Some(is_python_path),
                None,
                "Provided python path is not a valid Python executable",
            ) {
                config.set_str(ConfigKey::PythonPath, path);
            }
    }

    fn setup(&self) -> Option<HashMap<String, String>> {
        let Some(tracked_folders) = self.cli.tracked_folders.clone() else {
            error!("No tracked folders provided. Please provide at least one tracked folder using the --tracked-folders argument. Exiting.");
            return None;
        };
        info!("Using tracked folders: {:?}", tracked_folders);

        let ws_folders: HashMap<String, String> = tracked_folders
            .into_iter()
            .enumerate()
            .filter_map(|(id, path)| {
                canonicalize_and_validate(
                    &path,
                    Some(|_| true),
                    None,
                    "Unable to resolve tracked folder",
                )
                .map(|tf| (format!("{}", id), tf))
            })
            .collect();

        Some(ws_folders)
    }

    pub fn run(self) {
        let ws_folders = match self.setup() {
            Some(folders) => folders,
            None => return,
        };

        let mut server = SyncOdoo::new();
        let (s, r) = crossbeam_channel::unbounded();
        let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), None, &mut server);
        session.sync_odoo.load_odoo_addons = false;

        // Add workspace folders once
        for (id, tf) in &ws_folders {
            let uri = match FileMgr::try_pathname2uri(tf) {
                Ok(uri) => uri,
                Err(e) => {
                    error!("Unable to resolve tracked folder: {}, error: {}", tf, e);
                    continue;
                }
            };
            session
                .sync_odoo
                .get_file_mgr()
                .borrow_mut()
                .add_workspace_folder(id.clone(), uri);
        }

        // Load and reconcile configuration
        let mut config = match self.read_config_file(&mut session) {
            Some(config) => config,
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use assert_fs::TempDir;
    use crate::args::LogLevel;
    use crate::core::config::DEFAULT_PROFILE_NAME;
    use crate::utils::PathSanitizer;

    fn default_cli() -> Cli {
        Cli {
            parse: true,
            addons: None,
            community_path: None,
            tracked_folders: None,
            python: None,
            output: None,
            stubs: None,
            no_typeshed_stubs: false,
            stdlib: None,
            clientProcessId: None,
            use_tcp: false,
            log_level: LogLevel::INFO,
            logs_directory: None,
            config_path: None,
            selected_config: None,
        }
    }

    /// Helper function for tests to set up workspace folders and load config.
    fn setup_with_config(backend: &CliBackend) -> Option<(HashMap<String, String>, ConfigEntry)> {
        let ws_folders = backend.setup()?;

        // Create a session with workspace folders for config loading
        let mut server = SyncOdoo::new();
        let (s, r) = crossbeam_channel::unbounded();
        let mut session = SessionInfo::new_from_custom_channel(s, r, None, &mut server);

        // Add workspace folders to session
        for (id, path) in &ws_folders {
            if let Ok(uri) = FileMgr::try_pathname2uri(path) {
                session
                    .sync_odoo
                    .get_file_mgr()
                    .borrow_mut()
                    .add_workspace_folder(id.clone(), uri);
            }
        }

        // Load configuration
        let mut config = backend.read_config_file(&mut session)?;
        backend.reconcile_args_and_config_file(&mut config);

        Some((ws_folders, config))
    }

    // -------------------------------------------------------------------------
    // 1. Config TOML file is respected
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_toml_values_are_loaded() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "default"
                auto_refresh_delay = 9999
                file_cache = false
            "#,
            )
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("default")),
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend).expect("Expected a config entry to be returned");

        assert_eq!(config.auto_refresh_delay(), 9999);
        assert_eq!(config.file_cache(), false);
    }

    /// Without `--config-path`, setup should succeed and return the default `ConfigEntry`.
    #[test]
    fn test_no_config_path_returns_default_entry() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();
        let cli = Cli {
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend)
            .expect("Expected a default config entry when no config_path is set");

        // Default auto_refresh_delay is 1000 ms
        assert_eq!(config.auto_refresh_delay(), 1000);
    }

    // -------------------------------------------------------------------------
    // 2. Workspace folders (tracked_folders) are used as context
    // -------------------------------------------------------------------------

    /// `--tracked-folders` should be registered as workspace folders, allowing template
    /// variables like `${workspaceFolder:0}` inside the TOML to resolve to those paths.
    #[test]
    fn test_tracked_folders_resolve_workspace_template_variables() {
        let temp = TempDir::new().unwrap();

        // Create a workspace folder that qualifies as an addon path
        // (contains at least one sub-directory with __manifest__.py).
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();
        ws1.child("my_module").create_dir_all().unwrap();
        ws1.child("my_module")
            .child("__manifest__.py")
            .touch()
            .unwrap();

        // TOML placed outside ws1, referencing it via the workspace template variable.
        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "default"
                addons_paths = ["${workspaceFolder:0}"]
            "#,
            )
            .unwrap();

        // Pass ws1 directly as a tracked folder - setup() will register it and
        // use it to resolve ${workspaceFolder:0} inside the TOML.
        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("default")),
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend).expect("Expected config entry");

        let ws1_expected = fs::canonicalize(ws1.path()).unwrap().sanitize();

        assert!(
            config.addons_paths().contains(&ws1_expected),
            "Expected ws1 to be in addons_paths after resolving template variable, got: {:?}",
            config.addons_paths()
        );
    }

    /// Without `--tracked-folders`, `${workspaceFolder:ws1}` cannot be resolved,
    /// so no addon paths should appear.
    #[test]
    fn test_workspace_template_not_resolved_when_no_tracked_folders() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "default"
                addons_paths = ["${workspaceFolder:ws1}"]
            "#,
            )
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("default")),
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (ws_folders, config) = setup_with_config(&backend)
            .expect("Config should still load");

        assert!(!ws_folders.is_empty(), "Workspace folder should be registered with numeric ID");
        assert!(
            config.addons_paths().is_empty(),
            "Expected no addons_paths when template variable references non-existent named workspace, got: {:?}",
            config.addons_paths()
        );
    }

    /// Multiple `--tracked-folders` paths should all be registered as workspace folders.
    #[test]
    fn test_multiple_tracked_folders_are_all_registered() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        let ws2 = temp.child("ws2");
        ws1.create_dir_all().unwrap();
        ws2.create_dir_all().unwrap();

        let ws1_expected = fs::canonicalize(ws1.path()).unwrap().sanitize();
        let ws2_expected = fs::canonicalize(ws2.path()).unwrap().sanitize();

        let cli = Cli {
            tracked_folders: Some(vec![
                ws1.path().to_string_lossy().into_owned(),
                ws2.path().to_string_lossy().into_owned(),
            ]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (ws_folders, _) = setup_with_config(&backend).expect("Expected successful setup");

        assert_eq!(ws_folders.len(), 2, "Both tracked folders should be registered");
        assert!(
            ws_folders.values().any(|p| p == &ws1_expected),
            "ws1 should be registered"
        );
        assert!(
            ws_folders.values().any(|p| p == &ws2_expected),
            "ws2 should be registered"
        );
    }

    // -------------------------------------------------------------------------
    // 3. Selected config profile
    // -------------------------------------------------------------------------

    /// `--selected-config` should load the matching profile from the TOML.
    #[test]
    fn test_selected_config_profile_is_loaded() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "profile_a"
                auto_refresh_delay = 1111

                [[config]]
                name = "profile_b"
                auto_refresh_delay = 2222
            "#,
            )
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("profile_b")),
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend).expect("Expected successful setup");

        assert_eq!(
            config.auto_refresh_delay(), 2222,
            "profile_b should have auto_refresh_delay=2222"
        );
    }

    /// When `--selected-config` is omitted, setup must fall back to the default profile
    /// (`DEFAULT_PROFILE_NAME`) and succeed if that profile exists in the TOML.
    #[test]
    fn test_omitting_selected_config_uses_default_profile() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(&format!(
                r#"
                [[config]]
                name = "{name}"
                auto_refresh_delay = 4242
            "#,
                name = DEFAULT_PROFILE_NAME
            ))
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: None, // omitted — should fall back to DEFAULT_PROFILE_NAME
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend)
            .expect("Expected setup to succeed using the default profile as fallback");

        assert_eq!(
            config.auto_refresh_delay(), 4242,
            "Should have loaded the default profile's settings"
        );
    }

    /// When `--selected-config` is omitted AND the TOML has no default profile,
    /// setup must return `None`.
    #[test]
    fn test_omitting_selected_config_when_no_default_profile_yields_none() {
        let temp = TempDir::new().unwrap();
        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "custom_only"
                auto_refresh_delay = 1234
            "#,
            )
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: None, // omitted — fallback to default, which doesn't exist
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        assert!(
            setup_with_config(&backend).is_none(),
            "Expected None when the fallback default profile is absent from the TOML"
        );
    }

    /// If the named profile does not exist in the TOML, setup must return `None`.
    #[test]
    fn test_nonexistent_profile_yields_none() {
        let temp = TempDir::new().unwrap();
        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(r#"[[config]]
            name = "existing""#)
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("ghost_profile")),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        assert!(
            setup_with_config(&backend).is_none(),
            "Expected None when the selected profile doesn't exist in the config file"
        );
    }

    // -------------------------------------------------------------------------
    // 4. CLI args take precedence over config file values
    // -------------------------------------------------------------------------

    /// `--no-typeshed-stubs` on the CLI must override `no_typeshed_stubs = false` in the TOML.
    #[test]
    fn test_cli_no_typeshed_stubs_overrides_config_file() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "default"
                no_typeshed_stubs = false
            "#,
            )
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("default")),
            no_typeshed_stubs: true, // CLI override
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend).expect("Expected config entry");

        assert!(
            config.no_typeshed_stubs(),
            "CLI --no-typeshed-stubs=true must win over config file's no_typeshed_stubs=false"
        );
    }

    /// `--addons` on the CLI must extend the addons_paths from the config file,
    /// even when that config file contains no addon entries of its own.
    #[test]
    fn test_cli_addons_arg_extends_config_file_addons() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        // An addons directory: must contain a sub-directory with __manifest__.py
        // so that `is_addon_path` returns true.
        let addons_dir = temp.child("extra_addons");
        addons_dir.create_dir_all().unwrap();
        addons_dir.child("mod_x").create_dir_all().unwrap();
        addons_dir
            .child("mod_x")
            .child("__manifest__.py")
            .touch()
            .unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(
                r#"
                [[config]]
                name = "default"
            "#,
            )
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("default")),
            addons: Some(vec![addons_dir.path().to_string_lossy().into_owned()]),
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend).expect("Expected config entry");

        let addons_expected = fs::canonicalize(addons_dir.path()).unwrap().sanitize();

        assert!(
            config.addons_paths().contains(&addons_expected),
            "CLI --addons should add the directory to addons_paths, got: {:?}",
            config.addons_paths()
        );
    }

    /// When both the TOML and CLI specify addons paths, the result should be the union.
    #[test]
    fn test_cli_addons_arg_and_config_addons_are_merged() {
        let temp = TempDir::new().unwrap();
        let ws1 = temp.child("ws1");
        ws1.create_dir_all().unwrap();

        // Addon dir referenced in the TOML
        let config_addons = temp.child("config_addons");
        config_addons.create_dir_all().unwrap();
        config_addons.child("mod_a").create_dir_all().unwrap();
        config_addons
            .child("mod_a")
            .child("__manifest__.py")
            .touch()
            .unwrap();

        // Addon dir passed only via CLI
        let cli_addons = temp.child("cli_addons");
        cli_addons.create_dir_all().unwrap();
        cli_addons.child("mod_b").create_dir_all().unwrap();
        cli_addons
            .child("mod_b")
            .child("__manifest__.py")
            .touch()
            .unwrap();

        let toml_path = temp.child("odools.toml");
        toml_path
            .write_str(&format!(
                r#"
                [[config]]
                name = "default"
                addons_paths = ["{config_addons_path}"]
            "#,
                config_addons_path = config_addons.path().sanitize().replace('\\', "/")
            ))
            .unwrap();

        let cli = Cli {
            config_path: Some(toml_path.path().to_string_lossy().into_owned()),
            selected_config: Some(S!("default")),
            addons: Some(vec![cli_addons.path().to_string_lossy().into_owned()]),
            tracked_folders: Some(vec![ws1.path().to_string_lossy().into_owned()]),
            ..default_cli()
        };
        let backend = CliBackend::new(cli);
        let (_, config) = setup_with_config(&backend).expect("Expected config entry");

        let config_addons_expected = fs::canonicalize(config_addons.path()).unwrap().sanitize();
        let cli_addons_expected = fs::canonicalize(cli_addons.path()).unwrap().sanitize();

        assert!(
            config.addons_paths().contains(&config_addons_expected),
            "addons from config file must be present, got: {:?}",
            config.addons_paths()
        );
        assert!(
            config.addons_paths().contains(&cli_addons_expected),
            "addons from CLI must be present, got: {:?}",
            config.addons_paths()
        );
    }
}
