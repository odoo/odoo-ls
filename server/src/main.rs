mod backend;
mod constants;
mod core;
use lazy_static::lazy_static;
mod my_weak;

use backend::Backend;
use core::odoo::Odoo;
use core::file_mgr::FileMgr;
use tokio::sync::Mutex;
use std::sync::Arc;
use tower_lsp::{LspService, Server};

lazy_static! {
    static ref FILE_MGR: Mutex<FileMgr> = Mutex::new(FileMgr::new());
}

#[tokio::main]
async fn main() {
    println!("starting server");
    let debug = true;
    if debug {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:2087").await.unwrap();

        //loop {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, writer) = tokio::io::split(stream);
        let (service, messages) = LspService::build(|client| Backend { client, odoo:Arc::new(Mutex::new(Odoo::new())) })
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
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(|client| Backend { client, odoo:Arc::new(Mutex::new(Odoo::new())) });
        Server::new(stdin, stdout, socket).serve(service).await;
    }
}