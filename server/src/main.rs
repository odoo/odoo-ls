use server::{args::Cli, cli_backend::CliBackend, server::Server};
use clap::Parser;
use server::core::odoo::Odoo;
use std::env;
use std::sync::Arc;

fn main() {
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
            let mut serv = Server::new_tcp().expect("Unable to start tcp connection");
            serv.initialize().expect("Error while initializing server");
            serv.run();
        } else {
            println!("starting server");
            let mut serv = Server::new_stdio();
            serv.initialize().expect("Error while initializing server");
            serv.run();
        }
    }
}