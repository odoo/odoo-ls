use lsp_server::Notification;
use serde_json::json;
use odoo_ls_server::{args::{Cli, LogLevel}, cli_backend::CliBackend, constants::*, server::Server, utils::PathSanitizer};
use clap::Parser;
use tracing::{info, level_filters::LevelFilter, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_panic::panic_hook;
use tracing_subscriber::{fmt, FmtSubscriber, layer::SubscriberExt};

use std::{env, path::PathBuf};

use iai_callgrind::{
    library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig,
    FlamegraphConfig
};
use std::hint::black_box;

/*
To run iai-callgrind:
install valgrind
run cargo install --version 0.14.0 iai-callgrind-runner
*/

#[library_benchmark]
fn iai_main() {
    env::set_var("RUST_BACKTRACE", "full");

    tracing_subscriber::fmt::init();

    let COMMUNITY_PATH = env::var("COMMUNITY_PATH").unwrap_or("/home/odoo/Documents/odoo-servers/test-odoo/odoo".to_string());
    let mut server = odoo_ls_server::core::odoo::SyncOdoo::new();
    let (s, r) = crossbeam_channel::unbounded();
    let mut session = odoo_ls_server::threads::SessionInfo::new_from_custom_channel(s.clone(), r.clone(), &mut server, None);
    session.sync_odoo.load_odoo_addons = false;

    let addons_paths = vec![];
    info!("Using addons path: {:?}", addons_paths);

    let workspace_folders = vec![COMMUNITY_PATH.clone()];
    info!("Using tracked folders: {:?}", workspace_folders);

    for tracked_folder in workspace_folders {
        session.sync_odoo.get_file_mgr().borrow_mut().add_workspace_folder(PathBuf::from(tracked_folder).sanitize());
    }

    let mut config = odoo_ls_server::core::config::Config::new();
    config.addons = addons_paths;
    config.odoo_path = COMMUNITY_PATH;
    config.python_path = odoo_ls_server::S!("python3");
    config.refresh_mode = odoo_ls_server::core::config::RefreshMode::Off;
    config.diag_missing_imports = odoo_ls_server::core::config::DiagMissingImportsMode::All;
    config.no_typeshed = false;
    config.additional_stubs = vec![];
    config.stdlib = "".to_string();

    black_box(odoo_ls_server::core::odoo::SyncOdoo::init(&mut session, config));
    info!(">>>>>>>>>>>>>>>>>> End Session <<<<<<<<<<<<<<<<<<");
}

library_benchmark_group!(name = my_group; benchmarks = iai_main);

main!(
    config = LibraryBenchmarkConfig::default().flamegraph(FlamegraphConfig::default());
    library_benchmark_groups = my_group
);