use lsp_server::Message;
use odoo_ls_server::{
    core::{config::ConfigEntry, odoo::SyncOdoo},
    threads::SessionInfo,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use super::create_init_session;

/// Runs a list of test cases (functions) that have the same signature.
/// A panic in a test case does not prevent the other cases to run.
pub fn run_no_fail_fast<T>(cases: impl IntoIterator<Item = T>, mut call: impl FnMut(T)) {
    let mut failed = 0;
    for case in cases {
        if catch_unwind(AssertUnwindSafe(|| call(case))).is_err() {
            failed += 1;
        }
    }
    assert!(failed == 0, "{failed} test cases failed, see the panic messages above");
}

/// Builds the server once, then runs each case on its own isolated copy.
/// Copying the server means: clone SyncOdoo + clone Session's messages
pub fn build_once_run_isolated(
    mut odoo: SyncOdoo,
    config: ConfigEntry,
    cases: &[&dyn Fn(SessionInfo)],
) {
    let messages: Vec<Message> = {
        // The server builds the database and issues diagnostic messages
        let session = create_init_session(&mut odoo, config);
        std::iter::from_fn(|| session._consume_message()).collect()
    };
    let last = cases.len().saturating_sub(1);
    run_no_fail_fast(cases.iter().enumerate(), |(i, case)| {
        let mut odoo_clone;
        // Clone SyncOdoo unless it's the last run
        let odoo_ref = if i == last {
            &mut odoo
        } else {
            odoo_clone = odoo.clone();
            &mut odoo_clone
        };
        // Session with cloned messages
        case(session_with_messages(odoo_ref, &messages));
    });
}

fn session_with_messages<'a>(odoo: &'a mut SyncOdoo, messages: &[Message]) -> SessionInfo<'a> {
    let (s, r) = crossbeam_channel::unbounded();
    for message in messages {
        s.send(message.clone()).unwrap();
    }
    SessionInfo::new_from_custom_channel(s, r, None, odoo)
}
