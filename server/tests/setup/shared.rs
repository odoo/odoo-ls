#![allow(dead_code)]
//! A server that is initialized once and shared by every test of a test binary.
//!
//! `SyncOdoo` is not `Send`, so it can't sit in a static. Instead, each shared server lives on its
//! own worker thread and tests send it closures to run. Tests that share a server run one after
//! the other on that thread. Afterwards, the files the test opened are closed the way the editor
//! would, its entry points are dropped and the workspace files it changed are reloaded from disk,
//! so the next test starts from the initialized state.
//!
//! The diagnostics published while the server initializes are kept, and replayed into the
//! session of each test, so `get_diagnostics_for_path` still sees them.
//!
//! Only use it for tests that don't change the server's configuration or the files on disk. Tests
//! that do should keep building their own server with `setup_server` + `create_init_session`.

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Once, OnceLock};
use std::thread;

use crossbeam_channel::{Sender, unbounded};
use lsp_server::Message;
use lsp_types::notification::{Notification, PublishDiagnostics};
use lsp_types::{DidCloseTextDocumentParams, TextDocumentIdentifier, Uri};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::S;
use odoo_ls_server::threads::SessionInfo;

use super::setup::{create_init_session, setup_server};

type Job = Box<dyn FnOnce(&mut SessionInfo) + Send>;
/// `Err` holds the panic message of the test, with the location of the panic.
type JobResult = Result<(), String>;

static WITH_ODOO: OnceLock<Sender<(Job, Sender<JobResult>)>> = OnceLock::new();
static WITHOUT_ODOO: OnceLock<Sender<(Job, Sender<JobResult>)>> = OnceLock::new();

const WORKER_THREAD_PREFIX: &str = "shared-server";

thread_local! {
    static PANIC_LOCATION: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Run `test` on the shared server of this test binary, initializing it on the first call.
/// `with_odoo` has the same meaning as in [`setup_server`].
/// A panic inside `test` fails the calling test with the same message.
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
        // The worker inherited the output capture of the test that spawned it, so the message the
        // panic hook printed there is lost: panic again here, where the test harness shows it.
        Ok(Err(message)) => panic!("{message}"),
        Err(_) => panic!("shared test server stopped while running the test"),
    }
}

fn spawn_server(with_odoo: bool) -> Sender<(Job, Sender<JobResult>)> {
    install_panic_location_hook();
    let (job_sender, job_receiver) = unbounded::<(Job, Sender<JobResult>)>();
    let name = if with_odoo { format!("{WORKER_THREAD_PREFIX}-odoo") } else { WORKER_THREAD_PREFIX.to_string() };
    thread::Builder::new()
        .name(name)
        .spawn(move || {
            let (mut odoo, init_diagnostics) = init_server(with_odoo);
            for (job, result_sender) in job_receiver {
                let result = run_job(&mut odoo, &init_diagnostics, job);
                let _ = result_sender.send(result);
            }
        })
        .expect("unable to spawn the shared test server thread");
    job_sender
}

/// The panic payload only holds the message: keep the location of panics on the worker threads,
/// so the calling test can report it too.
fn install_panic_location_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if thread::current().name().is_some_and(|name| name.starts_with(WORKER_THREAD_PREFIX)) {
                let location = info.location().map(|location| location.to_string());
                PANIC_LOCATION.with(|cell| *cell.borrow_mut() = location);
            }
            previous_hook(info);
        }));
    });
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
    // Also after a panic, so `should_panic` tests don't leak their entries into the next test.
    session.sync_odoo.version = version;
    close_test_entries(&mut session);
    result.map_err(|payload| {
        let message = payload.downcast_ref::<&str>().map(|message| message.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| S!("<non-string panic payload>"));
        match PANIC_LOCATION.with(|cell| cell.borrow_mut().take()) {
            Some(location) => format!("{message}\n  (panicked at {location})"),
            None => message,
        }
    })
}

/// Undo what the test opened, so the next test starts from the initialized state: close its
/// files the way the editor would, drop the custom and untitled entry points it created, and
/// reload from disk the workspace files it changed.
fn close_test_entries(session: &mut SessionInfo) {
    let mut to_reload = close_opened_files(session);
    to_reload.extend(drop_test_entry_points(session));
    for path in to_reload {
        reload_from_disk(session, &path);
    }
}

/// Close every file the test opened. Return the ones still known to the server afterwards,
/// whose content may differ from the disk.
fn close_opened_files(session: &mut SessionInfo) -> Vec<String> {
    let file_mgr = session.sync_odoo.get_file_mgr();
    let mut opened = session.sync_odoo.opened_files.clone();
    for (path, file_info) in file_mgr.borrow().files.iter() {
        if file_info.borrow().opened && !opened.contains(path) {
            opened.push(path.clone());
        }
    }
    for path in opened.iter() {
        let uri = if FileMgr::is_untitled(path) {
            Uri::from_str(path).expect("invalid untitled uri")
        } else {
            FileMgr::pathname2uri(path)
        };
        Odoo::handle_did_close(session, DidCloseTextDocumentParams { text_document: TextDocumentIdentifier { uri } });
    }
    opened.into_iter()
        .filter(|path| !FileMgr::is_untitled(path) && file_mgr.borrow().get_file_info(path).is_some())
        .collect()
}

/// Drop the custom and untitled entry points left, with the files only they use.
/// Return the files that are also part of another entry point, which must be kept.
fn drop_test_entry_points(session: &mut SessionInfo) -> Vec<String> {
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
    let (to_keep, to_delete): (Vec<String>, Vec<String>) = paths.into_iter().partition(|path| {
        entry_point_mgr.borrow().iter_all_but_public().any(|entry| path.starts_with(&entry.borrow().path))
    });
    for path in to_delete {
        FileMgr::delete_file_path(session, &path);
    }
    to_keep
}

/// Load `path` from disk again, and rebuild its symbols if the content the test left differs.
fn reload_from_disk(session: &mut SessionInfo, path: &str) {
    let (updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path, None, Some(-100), false);
    file_info.borrow_mut().publish_diagnostics(session);
    if updated {
        SyncOdoo::unload_path(session, Path::new(path));
        Odoo::on_new_path(session, path);
        BuildScheduler::process_rebuilds(session, false);
    }
}
