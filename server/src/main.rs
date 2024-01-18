mod backend;
mod odoo;

use backend::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let debug = true;
    if debug {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:2087").await.unwrap();

        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, writer) = tokio::io::split(stream);
            let (service, messages) = LspService::build(|client| Backend { client, odoo:None })
                .custom_method("Odoo/configurationChanged", Backend::client_config_changed)
                .custom_method("Odoo/clientReady", Backend::client_ready)
                .finish();
            let server = Server::new(reader, writer, messages);
            tokio::spawn(async move {
                server.serve(service).await;
            });
        }
    } else {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(|client| Backend { client, odoo:None });
        Server::new(stdin, stdout, socket).serve(service).await;
    }
}