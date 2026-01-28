use core::str;
use std::collections::HashMap;
use std::{env, fs};

use std::path::PathBuf;


use lsp_server::Message;
use lsp_types::{Diagnostic, PublishDiagnosticsParams, TextDocumentContentChangeEvent};
use lsp_types::notification::{Notification, PublishDiagnostics};
use odoo_ls_server::S;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::utils::get_python_command;
use odoo_ls_server::{core::{config::{ConfigEntry, DiagMissingImportsMode}, entry_point::EntryPointMgr, odoo::SyncOdoo}, threads::SessionInfo, utils::PathSanitizer as _};

use tracing::{info, level_filters::LevelFilter};
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::{fmt, layer::SubscriberExt, FmtSubscriber};

pub fn setup_server(with_odoo: bool) -> (SyncOdoo, ConfigEntry) {

    let file_appender = RollingFileAppender::builder()
        .max_log_files(20) // only the most recent 5 log files will be kept
        .filename_prefix(format!("odoo_tests_logs_{}", std::process::id()))
        .filename_suffix("log")
        .build("./logs")
        .expect("failed to initialize rolling file appender");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = FmtSubscriber::builder()
        .with_thread_ids(true)
        .with_file(false)
        .with_max_level(LevelFilter::INFO)
        .with_ansi(false)
        .with_writer(file_writer)
        .finish();
    let stdout_subscriber = fmt::layer().with_writer(std::io::stdout).with_ansi(true);
    let _ = tracing::subscriber::set_global_default(subscriber.with(stdout_subscriber));


    let community_path = if with_odoo {
        Some(env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder"))
    } else {
        None
    };
    let mut server = SyncOdoo::new();
    server.load_odoo_addons = false;

    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path = test_addons_path.join("tests").join("data").join("addons");
    info!("Test addons path: {:?}", test_addons_path);

    let mut config = ConfigEntry::new();
    config.addons_paths = vec![test_addons_path.sanitize()].into_iter().collect();
    server.get_file_mgr().borrow_mut().add_workspace_folder(S!("test_addons_path"), test_addons_path.sanitize());
    config.odoo_path = community_path.map(|x| PathBuf::from(x).sanitize());
    let Some(python_cmd) = get_python_command() else {
        panic!("Python not found")
    };
    config.python_path = python_cmd;
    config.diag_missing_imports = DiagMissingImportsMode::All;
    (server, config)
}

pub fn create_init_session<'a>(odoo: &'a mut SyncOdoo, config: ConfigEntry) -> SessionInfo<'a> {
    let (s, r) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), odoo);
    session.sync_odoo.test_mode = true;
    SyncOdoo::init(&mut session, config);
    session
}

pub fn prepare_custom_entry_point(session: &mut SessionInfo, path: &str){
    let ep_path = PathBuf::from(path).sanitize();
    let text = fs::read_to_string(path).expect("unable to read provided path");
    let content = Some(vec![TextDocumentContentChangeEvent{
        range: None,
        range_length: None,
            text: text}]);
    EntryPointMgr::create_new_custom_entry_for_path(session, &ep_path, &ep_path);
    let (file_updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path, content.as_ref(), Some(1), false);
    SyncOdoo::process_rebuilds(session, false);
}

pub fn get_diagnostics_for_path(session: &mut SessionInfo, path: &str) -> Vec<Diagnostic> {
    let mut res = vec![];
    while let Some(msg) = session._consume_message() {
        match msg {
            Message::Notification(n) => {
                if n.method == PublishDiagnostics::METHOD {
                    let params: PublishDiagnosticsParams = serde_json::from_value(n.params).expect("Unable to parse PublishDiagnosticsParams");
                    let params_path = FileMgr::uri2pathname(params.uri.as_str());
                    if params_path == path {
                        res.extend(params.diagnostics);
                    }
                }
            },
            _ => {}
        }
    }
    return res;
}

pub fn get_diagnostics_for_paths(session: &mut SessionInfo, paths: &Vec<String>) -> HashMap<String, Vec<Diagnostic>> {
    let mut res = HashMap::new();
    while let Some(msg) = session._consume_message() {
        match msg {
            Message::Notification(n) => {
                if n.method == PublishDiagnostics::METHOD {
                    let params: PublishDiagnosticsParams = serde_json::from_value(n.params).expect("Unable to parse PublishDiagnosticsParams");
                    let params_path = FileMgr::uri2pathname(params.uri.as_str());
                    if paths.contains(&params_path) {
                        res.entry(params_path).or_insert_with(Vec::new).extend(params.diagnostics);
                    }
                }
            },
            _ => {}
        }
    }
    return res;
}

pub fn get_diagnostics_test_comments(session: &mut SessionInfo, path: &str) -> Vec<(u32, Vec<String>)> {
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_mgr = file_mgr.borrow();
    let file_info = file_mgr.get_file_info(&S!(path)).expect("File info not found");
    let file_info = file_info.borrow();
    file_info.diag_test_comments.clone()
}