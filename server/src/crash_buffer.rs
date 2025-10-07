
use lsp_server::Message;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::OnceLock;

const N: usize = 20;

pub static CRASH_BUFFER: OnceLock<Mutex<VecDeque<Message>>> = OnceLock::new();

pub fn init_crash_buffer() {
    let _ = CRASH_BUFFER.set(Mutex::new(VecDeque::with_capacity(N)));
}

pub fn push_message(msg: Message) {
    if let Some(buffer) = CRASH_BUFFER.get() {
        let mut buf = buffer.lock().unwrap();
        if buf.len() == N { buf.pop_front(); }
        buf.push_back(msg);
    }
}

pub fn get_messages() -> Vec<Message> {
    if let Some(buffer) = CRASH_BUFFER.get() {
        buffer.lock().unwrap().iter().cloned().collect()
    } else {
        Vec::new()
    }
}
