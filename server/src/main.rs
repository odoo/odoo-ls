use server::{args::Cli, cli_backend::CliBackend};
use clap::Parser;
use server::backend::Backend;
use server::core::odoo::Odoo;
use std::env;
use std::sync::Arc;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    env::set_var("RUST_BACKTRACE", "full");
    let cli = Cli::parse();
    let debug = true;

    if cli.parse {
        println!("starting server (single parse mode)");
        let backend = CliBackend::new(cli);
        backend.run();
    } else {
        if debug {
            println!("starting server (debug mode)");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:2087").await.unwrap();

            //loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, writer) = tokio::io::split(stream);
            let (sx, rx) = tokio::sync::mpsc::channel(1000);
            let (service, messages) = LspService::build(|client| Backend { client, odoo:Arc::new(tokio::sync::Mutex::new(Odoo::new(sx))), msg_receiver: Arc::new(tokio::sync::Mutex::new(rx)) })
                .custom_method("Odoo/configurationChanged", Backend::client_config_changed)
                .custom_method("Odoo/clientReady", Backend::client_ready)
                .finish();
            let server = Server::new(reader, writer, messages);
            server.serve(service).await;
                // tokio::spawn(async move {
                //     server.serve(service).await;
                // });
            //}
        } else {
            println!("starting server");
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();

            let (sx, rx) = tokio::sync::mpsc::channel(1000);
            let (service, socket) = LspService::build(|client| Backend { client, odoo:Arc::new(tokio::sync::Mutex::new(Odoo::new(sx))), msg_receiver: Arc::new(tokio::sync::Mutex::new(rx)) })
                .custom_method("Odoo/configurationChanged", Backend::client_config_changed)
                .custom_method("Odoo/clientReady", Backend::client_ready)
                .finish();
            Server::new(stdin, stdout, socket).serve(service).await;
        }
    }
}