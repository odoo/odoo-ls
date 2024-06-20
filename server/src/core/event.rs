use std::{path::PathBuf, sync::Arc};

use tokio::{sync::Mutex, task};
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

use super::odoo::Odoo;


#[derive(Debug)]
pub struct CreateFileEvent {
    time: std::time::Instant,
    force_process: bool
}

impl CreateFileEvent {
    pub fn new() -> Self {
        Self {
            time: std::time::Instant::now(),
            force_process: false
        }
    }
}

#[derive(Debug)]
pub struct UpdateFileEvent {
    time: std::time::Instant,
    force_process: bool
}

impl UpdateFileEvent {
    pub fn new() -> Self {
        Self {
            time: std::time::Instant::now(),
            force_process: false
        }
    }
}

#[derive(Debug)]
pub struct DeleteFileEvent {
    time: std::time::Instant,
    force_process: bool
}

impl DeleteFileEvent {
    pub fn new() -> Self {
        Self {
            time: std::time::Instant::now(),
            force_process: false
        }
    }
}

#[derive(Debug)]
pub struct OpenFileEvent {
    time: std::time::Instant,
    force_process: bool,
    path: PathBuf,
    content: String,
    version: i32,
}

impl OpenFileEvent {
    pub fn new(path: PathBuf, content: String, version: i32) -> Self {
        Self {
            time: std::time::Instant::now(),
            force_process: true,
            path: path,
            content: content,
            version: version
        }
    }

    pub fn process(&self, odoo: Arc<Mutex<Odoo>>) {
        let path = self.path.clone();
        let content = self.content.clone();
        let version = self.version.clone();
        task::spawn(async move {
            odoo.lock().await.reload_file(path, vec![TextDocumentContentChangeEvent{
                range: None,
                range_length: None,
                text: content}],
            version, true).await;
        });
    }
}

#[derive(Debug)]
pub struct CloseFileEvent {
    time: std::time::Instant,
    force_process: bool
}

impl CloseFileEvent {
    pub fn new() -> Self {
        Self {
            time: std::time::Instant::now(),
            force_process: false
        }
    }
}

#[derive(Debug)]
pub enum UpdateEvent {
    CREATE_FILE(CreateFileEvent),
    UPDATE_FILE(UpdateFileEvent),
    DELETE_FILE(DeleteFileEvent),
    OPEN_FILE(OpenFileEvent),
    CLOSE_FILE(CloseFileEvent),
}

impl UpdateEvent {
    pub fn process(&mut self) {

    }

    pub fn get_time(&self) -> &std::time::Instant {
        match self {
            UpdateEvent::CREATE_FILE(c) => {
                &c.time
            },
            UpdateEvent::UPDATE_FILE(c) => {
                &c.time
            },
            UpdateEvent::DELETE_FILE(c) => {
                &c.time
            },
            UpdateEvent::OPEN_FILE(c) => {
                &c.time
            },
            UpdateEvent::CLOSE_FILE(c) => {
                &c.time
            }
        }
    }

    pub fn set_time(&mut self, instant: std::time::Instant) {
        match self {
            UpdateEvent::CREATE_FILE(c) => {
                c.time = instant;
            },
            UpdateEvent::UPDATE_FILE(c) => {
                c.time = instant;
            },
            UpdateEvent::DELETE_FILE(c) => {
                c.time = instant;
            },
            UpdateEvent::OPEN_FILE(c) => {
                c.time = instant;
            },
            UpdateEvent::CLOSE_FILE(c) => {
                c.time = instant;
            }
        }
    }

    pub fn must_force_process(&self) -> bool {
        match self {
            UpdateEvent::CREATE_FILE(c) => {
                c.force_process
            },
            UpdateEvent::UPDATE_FILE(c) => {
                c.force_process
            },
            UpdateEvent::DELETE_FILE(c) => {
                c.force_process
            },
            UpdateEvent::OPEN_FILE(c) => {
                c.force_process
            },
            UpdateEvent::CLOSE_FILE(c) => {
                c.force_process
            }
        }
    }
}