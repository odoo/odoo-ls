#![allow(dead_code)]
//! A server that is initialized once and shared by every test of a test binary.
//!
//! `SyncOdoo` is not `Send`, so it can't sit in a static. Instead, each shared server lives on its
//! own worker thread and tests send it closures to run. Tests that share a server run one after
//! the other on that thread. Afterwards, the entry points and files the test opened are dropped,
//! the way closing them in the editor would, so the next test starts from the initialized state.
//!
//! The diagnostics published while the server initializes are kept, and replayed into the
//! session of each test, so `get_diagnostics_for_path` still sees them.
//!
//! Only use it for tests that don't change the server's configuration or the files on disk. Tests
//! that do should keep building their own server with `setup_server` + `create_init_session`.

use std::panic::{self, AssertUnwindSafe};
use std::sync::OnceLock;
use std::thread;

use crossbeam_channel::{Sender, unbounded};
use lsp_server::Message;
use lsp_types::notification::{Notification, PublishDiagnostics};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::threads::SessionInfo;

use super::setup::{create_init_session, setup_server};

type Job = Box<dyn FnOnce(&mut SessionInfo) + Send>;
type JobResult = thread::Result<()>;

static WITH_ODOO: OnceLock<Sender<(Job, Sender<JobResult>)>> = OnceLock::new();
static WITHOUT_ODOO: OnceLock<Sender<(Job, Sender<JobResult>)>> = OnceLock::new();

/// Run `test` on the shared server of this test binary, initializing it on the first call.
/// `with_odoo` has the same meaning as in [`setup_server`].
/// A panic inside `test` fails the calling test, and the server is rebuilt for the next one.
pub fn with_shared_session<F>(with_odoo: bool, test: F)
where
    F: FnOnce(&mut SessionInfo) + Send + 'static,
{
    let cell = if with_odoo { &WITH_ODOO } else { &WITHOUT_ODOO };
    let jobs = cell.get_or_init(|| spawn_server(with_odoo));
    let (result_sender, result_receiver) = unbounded();
    jobs.send((Box::new(test), result_sender)).expect("shared test server is gone");
    match result_receiver.recv() {
        Ok(Ok(())) => {}
        Ok(Err(panic_payload)) => panic::resume_unwind(panic_payload),
        Err(_) => panic!("shared test server stopped while running the test"),
    }
}

fn spawn_server(with_odoo: bool) -> Sender<(Job, Sender<JobResult>)> {
    let (job_sender, job_receiver) = unbounded::<(Job, Sender<JobResult>)>();
    let name = if with_odoo { "shared-server-odoo" } else { "shared-server" };
    thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let (mut odoo, mut init_diagnostics) = init_server(with_odoo);
            for (job, result_sender) in job_receiver {
                let result = run_job(&mut odoo, &init_diagnostics, job);
                if result.is_err() {
                    // The test stopped halfway, so the server may be in any state: start over.
                    (odoo, init_diagnostics) = init_server(with_odoo);
                }
                let _ = result_sender.send(result);
            }
        })
        .expect("unable to spawn the shared test server thread");
    job_sender
}

/// Return the initialized server and the diagnostics it published during init.
fn init_server(with_odoo: bool) -> (SyncOdoo, Vec<Message>) {
    let (mut odoo, config) = setup_server(with_odoo);
    let session = create_init_session(&mut odoo, config);
    let mut init_diagnostics = vec![];
    while let Some(message) = session._consume_message() {
        if let Message::Notification(notification) = &message
            && notification.method == PublishDiagnostics::METHOD {
                init_diagnostics.push(message);
            }
    }
    drop(session);
    (odoo, init_diagnostics)
}

fn run_job(odoo: &mut SyncOdoo, init_diagnostics: &[Message], job: Job) -> JobResult {
    // A new session per test, so the messages a test reads come from that test only.
    let (sender, receiver) = unbounded();
    for message in init_diagnostics {
        sender.send(message.clone()).expect("unable to replay the init diagnostics");
    }
    let mut session = SessionInfo::new_from_custom_channel(sender, receiver, None, odoo);
    let version = session.sync_odoo.version;
    let result = panic::catch_unwind(AssertUnwindSafe(|| job(&mut session)));
    if result.is_ok() {
        session.sync_odoo.version = version;
        close_test_entries(&mut session);
    }
    result
}

/// Drop the custom and untitled entry points the test created, with their files.
fn close_test_entries(session: &mut SessionInfo) {
    let entry_point_mgr = session.sync_odoo.entry_point_mgr.clone();
    let mut paths = vec![];
    {
        let mut entry_point_mgr = entry_point_mgr.borrow_mut();
        for entry in entry_point_mgr.custom_entry_points.iter().chain(entry_point_mgr.untitled_entry_points.iter()) {
            entry.borrow_mut().to_delete = true;
            paths.push(entry.borrow().path.clone());
        }
        entry_point_mgr.clean_entries(&mut session.sync_odoo.symbol_table);
    }
    session.sync_odoo.opened_files.clear();
    let file_mgr = session.sync_odoo.get_file_mgr();
    let opened_files: Vec<String> = file_mgr.borrow().files.iter()
        .filter(|(_, file_info)| file_info.borrow().opened)
        .map(|(path, _)| path.clone())
        .collect();
    for path in opened_files {
        if let Some(file_info) = file_mgr.borrow().get_file_info(&path) {
            let mut file_info = file_info.borrow_mut();
            file_info.opened = false;
            file_info.version = None;
        }
        paths.push(path);
    }
    for path in paths {
        FileMgr::delete_file_path(session, &path);
    }
}
