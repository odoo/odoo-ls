use std::env;
use std::path::Path;
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

// Regression test: `PythonArchBuilder::load_arch` used to unwrap a `FileMgr` cache
// entry that could already be evicted, panicking.
//
// Why: the same on-disk path can back two symbols (a relative import resolves
// against the importer's own entry root, so a separate entry over the same
// directory gets its own duplicate). Finishing one duplicate's ARCH_EVAL evicts the
// shared cached AST; if the other, internal duplicate still has a deferred method
// build pending, that build later hits the missing cache.
//
// Fixture: `pkg/b.py` (`Thing.method`), `pkg/__init__.py` (imports + calls it,
// internal), `pkg/c.py` (same import, but its own entry, so it's an external
// duplicate of `b.py`). Queuing both entries before `process_rebuilds()` lets the
// scheduler build the duplicate first, evict the cache, and skip the real `b.py`'s
// turn as a duplicate-looking no-op ("Already arch eval rebuilt, skipping") — so
// when `pkg.__init__` later forces `b.py`'s method to build, the cache is gone.
// Mirrors an editor batching several `.venv` file creates into one
// `didChangeWatchedFiles`. Not reproducible via `Odoo::handle_did_create` directly
// — its per-file loop finishes each file before the next starts.
#[test]
fn test_dependency_queued_alongside_its_external_duplicate_does_not_panic() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let root = env::current_dir().unwrap().join("tests/data/queued_import_race").sanitize();
    let pkg_dir = Path::new(&root).join("pkg").sanitize();
    let pkg_init = Path::new(&pkg_dir).join("__init__.py").sanitize();
    let c_path = Path::new(&pkg_dir).join("c.py").sanitize();

    // Register c.py's entry BEFORE pkg's, and both before any process_rebuilds()
    // call, so both land in the ARCH queue together and get drained by the single
    // process_rebuilds() call below — nothing here pokes build state directly,
    // this is the same EntryPointMgr::create_new_custom_entry_for_path call
    // handle_did_create itself makes per file.
    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &c_path, &c_path);
    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &pkg_dir, &pkg_init);
    BuildScheduler::process_rebuilds(&mut session, false);

    let method = session
        .sync_odoo
        .get_symbol(&pkg_dir, (&["b"], &["Thing", "method"]), u32::MAX)
        .first()
        .expect("expected a function symbol for pkg.b.Thing.method")
        .unwrap_function_key();

    assert!(!session.st().is_external(method.into()), "pkg.b is a real, internal module");
    assert!(!session.st()[method].evaluations.is_empty(), "method should have been built successfully");
}
