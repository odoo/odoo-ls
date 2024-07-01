use server::{args::Cli, cli_backend::CliBackend, server::Server};
use clap::Parser;
use tracing::{info, Level, error};
use tracing_panic::panic_hook;
use tracing_subscriber::{fmt, FmtSubscriber, layer::SubscriberExt};
use server::core::odoo::Odoo;
use std::env;
use std::sync::Arc;

fn main() {
    env::set_var("RUST_BACKTRACE", "full");
    let cli = Cli::parse();
    let debug = true;

    let file_appender = tracing_appender::rolling::hourly("./logs", "odoo_logs.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let file_subscriber = fmt::layer().with_writer(file_writer).with_ansi(false);
    let subscriber = FmtSubscriber::builder()
        .with_thread_ids(true)
        .with_file(false)
        .with_max_level(Level::TRACE)
        .with_ansi(true)
        .finish().with(file_subscriber);
    tracing::subscriber::set_global_default(subscriber).expect("Unable to set default tracing subscriber");

    std::panic::set_hook(Box::new(move |panic_info| {
        panic_hook(panic_info);
    }));

    

    info!(">>>>>>>>>>>>>>>>>> New Session <<<<<<<<<<<<<<<<<<");

    if cli.parse {
        info!("starting server (single parse mode)");
        let backend = CliBackend::new(cli);
        backend.run();
    } else {
        if debug {
            info!(tag = "test", "starting server (debug mode)");
            let mut serv = Server::new_tcp().expect("Unable to start tcp connection");
            serv.initialize().expect("Error while initializing server");
            serv.run(cli.clientProcessId);
        } else {
            info!("starting server");
            let mut serv = Server::new_stdio();
            serv.initialize().expect("Error while initializing server");
            serv.run(cli.clientProcessId);
        }
    }
    info!(">>>>>>>>>>>>>>>>>> End Session <<<<<<<<<<<<<<<<<<");
}