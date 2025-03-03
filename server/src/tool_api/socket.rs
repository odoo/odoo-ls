//from lsp_server crate
use std::{
    io::{self, BufReader},
    net::TcpStream,
    thread,
};

use crossbeam_channel::{bounded, Receiver, Sender};

use lsp_server::{Message};

use super::io_threads::ToolAPIIoThreads;

pub(crate) fn socket_transport(
    stream: TcpStream,
) -> (Sender<Message>, Receiver<Message>, ToolAPIIoThreads) {
    let (reader_receiver, reader) = make_reader(stream.try_clone().unwrap());
    let (writer_sender, writer) = make_write(stream);
    let io_threads = ToolAPIIoThreads{ reader, writer};
    (writer_sender, reader_receiver, io_threads)
}

fn make_reader(stream: TcpStream) -> (Receiver<Message>, thread::JoinHandle<io::Result<()>>) {
    let (reader_sender, reader_receiver) = bounded::<Message>(0);
    let reader = thread::spawn(move || {
        let mut buf_read = BufReader::new(stream);
        while let Some(msg) = match Message::read(&mut buf_read) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Error reading message: {}", e);
                None
            }
        } {
            let is_exit = matches!(&msg, Message::Notification(n) if n.method == "exit");
            reader_sender.send(msg).unwrap();
            if is_exit {
                break;
            }
        }
        Ok(())
    });
    (reader_receiver, reader)
}

fn make_write(mut stream: TcpStream) -> (Sender<Message>, thread::JoinHandle<io::Result<()>>) {
    let (writer_sender, writer_receiver) = bounded::<Message>(0);
    let writer = thread::spawn(move || {
        writer_receiver.into_iter().try_for_each(|it| it.write(&mut stream)).unwrap();
        Ok(())
    });
    (writer_sender, writer)
}
