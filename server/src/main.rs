use lsp_server::Notification;
use serde_json::json;
use odoo_ls_server::{args::{Cli, LogLevel}, cli_backend::CliBackend, constants::*, server::Server, utils::PathSanitizer, crash_buffer};
use clap::Parser;
use tracing::{info, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_panic::panic_hook;
use tracing_subscriber::{fmt, FmtSubscriber, layer::SubscriberExt};

use std::{env, path::PathBuf, process};
use odoo_ls_server::allocator::TrackingAllocator;

#[cfg(debug_assertions)]
#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

fn main() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::set_var("RUST_BACKTRACE", "full") };
    crash_buffer::init_crash_buffer();
    let cli = Cli::parse();

    let use_debug = cli.use_tcp;
    let log_level = &cli.log_level;
    let log_level = match log_level {
        LogLevel::TRACE => Level::TRACE,
        LogLevel::DEBUG => Level::DEBUG,
        LogLevel::INFO => Level::INFO,
        LogLevel::WARN => Level::WARN,
        LogLevel::ERROR => Level::ERROR,
    };

    let mut exe_dir = env::current_exe().expect("Unable to get binary directory... aborting");
    exe_dir.pop();

    let mut log_dir = exe_dir.join("logs").sanitize();
    if let Some(log_directory) = cli.logs_directory.clone() {
        let pathbuf = PathBuf::from(log_directory);
        if pathbuf.exists() {
            log_dir = pathbuf.sanitize();
        } else {
            println!("Given log directory path is invalid, fallbacking to default directory {}", log_dir);
        }
    }

    let file_appender = RollingFileAppender::builder()
        .max_log_files(5) // only the most recent 5 log files will be kept
        .rotation(Rotation::HOURLY)
        .filename_prefix("odoo_logs")
        .filename_suffix(format!("{}.log", std::process::id()))
        .build(log_dir)
        .expect("failed to initialize rolling file appender");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = FmtSubscriber::builder()
        .with_thread_ids(true)
        .with_file(false)
        .with_max_level(log_level)
        .with_ansi(false)
        .with_writer(file_writer)
        .finish();
    if cli.parse || use_debug {
        let stdout_subscriber = fmt::layer().with_writer(std::io::stdout).with_ansi(true);
        tracing::subscriber::set_global_default(subscriber.with(stdout_subscriber)).expect("Unable to set default tracing subscriber");
    } else {
        tracing::subscriber::set_global_default(subscriber).expect("Unable to set default tracing subscriber");
    }
    ctrlc::set_handler(move || {
        info!("Received ctrl-c signal");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    info!(">>>>>>>>>>>>>>>>>> New Session <<<<<<<<<<<<<<<<<<");
    info!("Server version: {}", EXTENSION_VERSION);
    info!("Compiled setting: DEBUG_ODOO_BUILDER: {}", DEBUG_ODOO_BUILDER);
    info!("Compiled setting: DEBUG_MEMORY: {}", DEBUG_MEMORY);
    info!("Compiled setting: DEBUG_THREADS: {}", DEBUG_THREADS);
    info!("Compiled setting: DEBUG_STEPS: {}", DEBUG_STEPS);
    info!("Compiled setting: DEBUG_REBUILD_NOW: {}", DEBUG_REBUILD_NOW);
    info!("Operating system: {}", std::env::consts::OS);
    info!("");

    if cli.parse {
        info!("starting server (single parse mode)");
        let backend = CliBackend::new(cli);
        backend.run();
    } else {
        let mut serv = if use_debug {
            info!(tag = "test", "starting server (debug mode)");
            Server::new_tcp().expect("Unable to start tcp connection")
        } else {
            info!("starting server");
            Server::new_stdio()
        };
        serv.initialize().expect("Error while initializing server");
        cli.config_path.map(|config_path| {
            serv.set_config_path(config_path.clone());
        });
        let sender_panic = serv.connection.as_ref().unwrap().sender.clone();
        std::panic::set_hook(Box::new(move |panic_info| {
            let backtrace = std::backtrace::Backtrace::capture();
            panic_hook(panic_info);
            let recent_msgs = crash_buffer::get_messages();
            let recent_msgs_json = serde_json::to_string_pretty(&recent_msgs).unwrap_or_default();
            let _ = sender_panic.send(lsp_server::Message::Notification(Notification{
                method: "Odoo/displayCrashNotification".to_string(),
                params: json!({
                    "crashInfo": format!("{panic_info}\n\nTraceback:\n{backtrace}"),
                    "recentMessages": recent_msgs_json,
                    "pid": std::process::id()
                })
            }));
        }));
        if !serv.run(cli.clientProcessId) {
            info!(">>>>>>>>>>>>>>>>>> End Session <<<<<<<<<<<<<<<<<<");
            process::exit(1);
        }
    }
    info!(">>>>>>>>>>>>>>>>>> End Session <<<<<<<<<<<<<<<<<<");
}
