use lsp_server::Notification;
use serde_json::json;
use odoo_ls_server::{args::{Cli, LogLevel}, cli_backend::CliBackend, constants::*, server::Server, utils::PathSanitizer};
use clap::Parser;
use tracing::{info, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_panic::panic_hook;
use tracing_subscriber::{fmt, FmtSubscriber, layer::SubscriberExt};

use std::{env, path::PathBuf};


fn main() {
    env::set_var("RUST_BACKTRACE", "full");
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
        .filename_prefix(format!("odoo_logs_{}", std::process::id()))
        .filename_suffix("log")
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
    info!("Operating system: {}", std::env::consts::OS);
    info!("");

    if cli.parse {
        info!("starting server (single parse mode)");
        let backend = CliBackend::new(cli);
        backend.run();
    } else if use_debug {
        info!(tag = "test", "starting server (debug mode)");
        let mut serv = Server::new_tcp().expect("Unable to start tcp connection");
        serv.initialize().expect("Error while initializing server");
        let sender_panic = serv.connection.as_ref().unwrap().sender.clone();
        std::panic::set_hook(Box::new(move |panic_info| {
            panic_hook(panic_info);
            let _ = sender_panic.send(lsp_server::Message::Notification(Notification{
                method: "Odoo/displayCrashNotification".to_string(),
                params: json!({
                    "crashInfo": format!("{panic_info}"),
                    "pid": std::process::id()
                })
            }));
        }));
        serv.run(cli.clientProcessId);
    } else {
        info!("starting server");
        let mut serv = Server::new_stdio();
        serv.initialize().expect("Error while initializing server");
        let sender_panic = serv.connection.as_ref().unwrap().sender.clone();
        std::panic::set_hook(Box::new(move |panic_info| {
            panic_hook(panic_info);
            let _ = sender_panic.send(lsp_server::Message::Notification(Notification{
                method: "Odoo/displayCrashNotification".to_string(),
                params: json!({
                    "crashInfo": format!("{panic_info}"),
                    "pid": std::process::id()
                })
            }));
        }));
        serv.run(cli.clientProcessId);
    }
    info!(">>>>>>>>>>>>>>>>>> End Session <<<<<<<<<<<<<<<<<<");
}