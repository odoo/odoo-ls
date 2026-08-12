use std::env;
use std::path::Path;

use lsp_types::{DidCloseTextDocumentParams, DidOpenTextDocumentParams, TextDocumentIdentifier, TextDocumentItem};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

// Closing a file whose symbols are shared with files that are still open must leave them
// alone. Regression: goto-definition on `str` opens builtins.pyi, and closing that tab
// unloaded builtins for the session - every later `get_ts_*` panicked on its unwrap.
#[test]
fn test_closing_builtins_pyi_keeps_builtins() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let builtins_path = Path::new(&session.sync_odoo.stdlib_dir).join("builtins.pyi").sanitize();
    assert!(
        !session.sync_odoo.get_symbol("", (&["builtins"], &["str"]), u32::MAX).is_empty(),
        "builtins.str must resolve before the close"
    );

    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: FileMgr::pathname2uri(&builtins_path) },
    });

    assert!(
        !session.sync_odoo.get_symbol("", (&["builtins"], &["str"]), u32::MAX).is_empty(),
        "builtins.str must survive a didClose on builtins.pyi"
    );
}

// A standalone file is still released on close, through its entry point rather than an
// unload. Its cache stays: the file is on disk, and nothing but its own entry served it.
#[test]
fn test_closing_a_standalone_file_still_releases_its_own_entry() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let point_path = env::current_dir().unwrap().join("tests/data/nested_dir_race/pkg/models/point.py").sanitize();
    let point_uri = FileMgr::pathname2uri(&point_path);
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: point_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: std::fs::read_to_string(&point_path).unwrap(),
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    assert!(!session.sync_odoo.get_symbol(&point_path, (&[], &["Thing"]), u32::MAX).is_empty());

    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: point_uri },
    });

    assert!(
        !session.ep_mgr().custom_entry_points.iter().any(|&ep| session.ep_mgr()[ep].path == point_path),
        "the entry point created for the buffer must go with it"
    );
    assert!(
        session.sync_odoo.get_symbol(&point_path, (&[], &["Thing"]), u32::MAX).is_empty(),
        "and so must its symbols"
    );
    assert!(
        session.file_mgr().get_file_info(&point_path).is_some(),
        "the file is still on disk, so its cache is kept"
    );
}

// The predicate behind that reclamation, including the empty case: a path that resolves
// to nothing is not external, or closing a standalone file would drop its cache too.
#[test]
fn test_is_external_path() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let builtins_path = Path::new(&session.sync_odoo.stdlib_dir).join("builtins.pyi").sanitize();
    assert!(session.sync_odoo.is_external_path(Path::new(&builtins_path)), "typeshed is external");

    let own_path = env::current_dir().unwrap().join("tests/data/nested_dir_race/pkg/models/point.py").sanitize();
    setup::setup::prepare_custom_entry_point(&mut session, &own_path);
    assert!(!session.sync_odoo.is_external_path(Path::new(&own_path)), "a file with an entry of its own is not");

    let unknown_path = env::current_dir().unwrap().join("tests/data/nothing_here.py").sanitize();
    assert!(!session.sync_odoo.is_external_path(Path::new(&unknown_path)), "and neither is a path that resolves to nothing");
}

// The one thing a close does reclaim: the cache of a file served only by a public entry
// (site-packages, sys.path). Arch-eval would have dropped it, but skips files that are open.
#[test]
fn test_closing_an_external_file_drops_its_cache_but_keeps_its_symbols() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let lib_dir = env::current_dir().unwrap().join("tests/data/external_entry").sanitize();
    let lib_path = Path::new(&lib_dir).join("lib_module.py").sanitize();
    let client_path = env::current_dir().unwrap().join("tests/data/external_entry_client.py").sanitize();
    EntryPointMgr::add_entry_to_public(&mut session, lib_dir.clone());
    setup::setup::prepare_custom_entry_point(&mut session, &client_path);

    assert!(
        session.sync_odoo.is_external_path(Path::new(&lib_path)),
        "imported through a public entry, so external"
    );

    let lib_uri = FileMgr::pathname2uri(&lib_path);
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: lib_uri.clone(),
            language_id: "python".to_string(),
            version: 1,
            text: std::fs::read_to_string(&lib_path).unwrap(),
        },
    });
    BuildScheduler::process_rebuilds(&mut session, false);
    assert!(session.file_mgr().get_file_info(&lib_path).is_some());

    Odoo::handle_did_close(&mut session, DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: lib_uri },
    });

    assert!(
        session.file_mgr().get_file_info(&lib_path).is_none(),
        "its cache is the thing a close is allowed to reclaim"
    );
    assert!(
        !session.sync_odoo.get_symbol(&client_path, (&["lib_module"], &["Thing"]), u32::MAX).is_empty(),
        "its symbols are not"
    );
}
