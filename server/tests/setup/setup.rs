use std::env;

use std::path::PathBuf;


use odoo_ls_server::{core::{config::{Config, DiagMissingImportsMode}, odoo::SyncOdoo}, threads::SessionInfo, utils::PathSanitizer as _};

use odoo_ls_server::S;
use tracing::{info, level_filters::LevelFilter};
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::{fmt, layer::SubscriberExt, FmtSubscriber};

pub fn setup_server() -> SyncOdoo {

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


    let community_path = env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder");
    info!("Community path: {:?}", community_path);
    let mut server = SyncOdoo::new();
    server.load_odoo_addons = false;

    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path = test_addons_path.join("tests").join("data").join("addons");
    info!("Test addons path: {:?}", test_addons_path);

    let mut config = Config::new();
    config.addons = vec![test_addons_path.sanitize()];
    config.odoo_path = community_path;
    config.python_path = S!("python3");
    config.refresh_mode = odoo_ls_server::core::config::RefreshMode::Off;
    config.diag_missing_imports = DiagMissingImportsMode::All;
    config.no_typeshed = false;

    let (s, r) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s, r, &mut server);
    SyncOdoo::init(&mut session, config);

    server
}