//! LSP work-done progress reporting.
//!
//! `start` opens the progress and returns the reporter; call `report_progress`
//! on that returned value as work advances, then drop it to close the progress:
//!
//! {
//!     let mut reporter = ProgressReporterRemaining::start(session, "Indexing");
//!     ...
//!     reporter.report_progress(remaining_items);
//!     ...
//! } // `reporter` goes out of scope here -> `End` is sent automatically.
//!
//!

use std::time::{Duration, Instant};
use crossbeam_channel::Sender;
use lsp_server::Message;
use lsp_types::notification::{Notification, Progress};
use lsp_types::{ProgressParams, ProgressParamsValue, ProgressToken, WorkDoneProgress, WorkDoneProgressBegin, WorkDoneProgressCreateParams, WorkDoneProgressEnd, WorkDoneProgressReport};
use lsp_types::request::{Request, WorkDoneProgressCreate};

use crate::threads::SessionInfo;

/// Minimum delay between two consecutive "remaining" report notifications.
const PROGRESS_REPORT_THROTTLE: Duration = Duration::from_millis(200);

/// Progress reporter that reports progress as a number of remaining items, throttled
pub struct ProgressReporterRemaining {
    notifier: Notifier,
    last_report_time: Instant,
}

impl ProgressReporterRemaining {
    #[must_use]
    pub fn start(session: &mut SessionInfo, title: &str) -> Self {
        Self {
            notifier: Notifier::start_reporting(session, None, title),
            // Far enough in the past that the first throttled report fires.
            last_report_time: Instant::now() - PROGRESS_REPORT_THROTTLE,
        }
    }

    pub fn report_progress(&mut self, remaining: usize) {
        let now = Instant::now();
        if now - self.last_report_time < PROGRESS_REPORT_THROTTLE {
            return;
        }
        self.last_report_time = now;
        self.notifier.send(WorkDoneProgress::Report(WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(format!("{remaining} items remaining")),
            percentage: None,
        }));
    }

    pub fn end(&mut self) {
        self.notifier.end();
    }
}

/// Progress reporter that reports progress as a percentage
pub struct ProgressReporterPercentage {
    notifier: Notifier,
    last_reported_progress: u32,
}

impl ProgressReporterPercentage {
    #[must_use]
    pub fn start(session: &mut SessionInfo, title: &str) -> Self {
        Self {
            notifier: Notifier::start_reporting(session, Some(0), title),
            last_reported_progress: 0,
        }
    }

    pub fn report_progress(&mut self, progress: u32) {
        if progress <= self.last_reported_progress || progress > 100 {
            return;
        }
        self.notifier.send(WorkDoneProgress::Report(WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(format!("{progress}%")),
            percentage: Some(progress),
        }));
        self.last_reported_progress = progress;
    }

    pub fn end(&mut self) {
        self.notifier.end();
    }
}


/// Communication layer for progress reporting
struct Notifier {
    progress_token: i32,
    sender: Sender<Message>,
}

impl Notifier {
    fn start_reporting(session: &mut SessionInfo, initial_percentage: Option<u32>, title: &str) -> Self {
        session.sync_odoo.progress_token += 1;
        let notifier = Self {
            progress_token: session.sync_odoo.progress_token,
            sender: session.clone_sender(),
        };
        let _ = session.send_request::<WorkDoneProgressCreateParams, ()>(WorkDoneProgressCreate::METHOD, WorkDoneProgressCreateParams {
            token: ProgressToken::Number(notifier.progress_token)
        });
        notifier.send(WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: title.to_string(),
            cancellable: Some(false),
            message: None,
            percentage: initial_percentage,
        }));
        notifier
    }

    fn end(&self) {
        self.send(WorkDoneProgress::End(WorkDoneProgressEnd {
            message: None,
        }))
    }

    fn send(&self, progress: WorkDoneProgress) {
        let Ok(params) = serde_json::to_value(ProgressParams {
            token: ProgressToken::Number(self.progress_token),
            value: ProgressParamsValue::WorkDone(progress),
        }) else {
            return;
        };
        let _ = self.sender.send(Message::Notification(lsp_server::Notification {
            method: Progress::METHOD.to_string(),
            params,
        }));
    }
}

/// Automatically sends an "End" notification when dropped.
impl Drop for Notifier {
    fn drop(&mut self) {
        self.send(WorkDoneProgress::End(WorkDoneProgressEnd { message: None }));
    }
}
