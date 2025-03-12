use std::{env, fs};

use std::path::PathBuf;


use lsp_types::TextDocumentContentChangeEvent;
use odoo_ls_server::{core::{config::{Config, DiagMissingImportsMode}, entry_point::EntryPointMgr, odoo::SyncOdoo}, threads::SessionInfo, utils::PathSanitizer as _};

use odoo_ls_server::S;
use tracing::{info, level_filters::LevelFilter};
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::{fmt, layer::SubscriberExt, FmtSubscriber};

pub fn setup_server(with_odoo: bool) -> SyncOdoo {

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
    tracing::subscriber::set_global_default(subscriber.with(stdout_subscriber)).expect("Unable to set default tracing subscriber");


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

    let mut config = Config::new();
    config.addons = vec![test_addons_path.sanitize()];
    config.odoo_path = community_path.map(|x| PathBuf::from(x).sanitize());
    config.python_path = S!("python");
    config.refresh_mode = odoo_ls_server::core::config::RefreshMode::Off;
    config.diag_missing_imports = DiagMissingImportsMode::All;
    config.no_typeshed = false;

    let (s, r) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s, r, &mut server);
    SyncOdoo::init(&mut session, config);

    server
}

pub fn create_session(odoo: &mut SyncOdoo) -> SessionInfo {
    let (s, r) = crossbeam_channel::unbounded();
    SessionInfo::new_from_custom_channel(s.clone(), r.clone(), odoo)
}

pub fn prepare_custom_entry_point<'a>(odoo: &'a mut SyncOdoo, path: &str) -> SessionInfo<'a>{
    let mut session = create_session(odoo);
    let ep_path = PathBuf::from(path).sanitize();
    let text = fs::read_to_string(path).expect("unable to read provided path");
    let content = Some(vec![TextDocumentContentChangeEvent{
        range: None,
        range_length: None,
            text: text}]);
    let (file_updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(&mut session, path, content.as_ref(), Some(1), false);
    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &ep_path);
    SyncOdoo::process_rebuilds(&mut session);
    session
}