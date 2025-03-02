use std::{io, thread};

pub struct ToolAPIIoThreads {
    pub reader: thread::JoinHandle<io::Result<()>>,
    pub writer: thread::JoinHandle<io::Result<()>>,
}

impl ToolAPIIoThreads {
    pub fn join(self) -> io::Result<()> {
        match self.reader.join() {
            Ok(r) => r?,
            Err(err) => {
                println!("reader panicked!");
                std::panic::panic_any(err)
            }
        }
        match self.writer.join() {
            Ok(r) => r,
            Err(err) => {
                println!("writer panicked!");
                std::panic::panic_any(err);
            }
        }
    }
}