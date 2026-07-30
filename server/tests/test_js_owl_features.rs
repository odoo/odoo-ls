//! Intergration tests of LSP features for JavaScript files and the OWL templates that back them.
//!
//! Most features require tsserver, so the suite is gated on a `TSSERVER` environment
//! variable holding the command that starts one:
//! 
//! `TSSERVER=tsserver cargo test --test test_js_owl_features`
//! 
//! Without it the test skips instead of failing, so `cargo test`
//! stays green on a machine with no TypeScript installed. `COMMUNITY_PATH` is required too.

use lsp_types::CompletionItemKind;
use odoo_ls_server::core::config::ConfigKey;
use odoo_ls_server::threads::SessionInfo;

mod setup;
mod js_owl_helpers;
use js_owl_helpers::fixture::*;
use js_owl_helpers::requests::*;
use js_owl_helpers::asserts::*;

/// The command to start tsserver with, or `None` when the suite should be skipped.
fn tsserver_command() -> Option<String> {
    match std::env::var("TSSERVER") {
        Ok(command) if !command.trim().is_empty() => Some(command),
        _ => None,
    }
}

#[test]
/// Test suite with a single server setup.
/// Requires env var `TSSERVER`, otherwise skipped.
fn test_js_owl_features() {
    let Some(tsserver_command) = tsserver_command() else {
        eprintln!("skipping test_js_owl_features: set TSSERVER to the tsserver command to run it");
        return;
    };

    let (mut odoo, mut config) = setup::setup::setup_server(true);
    config.set_str(ConfigKey::TsServerCommand, tsserver_command.clone());
    let (mut session, _tsserver_events) = setup::setup::create_init_session_with_tsserver(&mut odoo, config);

    // Check that tsserver started successfully
    assert!(
        session.sync_odoo.tsserver_bridge.is_some(),
        "tsserver did not start with TSSERVER={tsserver_command:?}"
    );

}
