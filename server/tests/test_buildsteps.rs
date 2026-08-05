use std::env;
use std::path::Path;

use odoo_ls_server::constants::{BuildStatus, BuildSteps};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::config::ConfigEntry;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::SymbolKey;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

//build a module with a single file
fn build_module(odoo: &mut SyncOdoo, config: ConfigEntry) -> (SessionInfo<'_>, String) {
    let mut session = setup::setup::create_init_session(odoo, config);

    let module_dir = env::current_dir()
        .unwrap()
        .join("tests/data/addons/module_buildsteps")
        .sanitize();
    let init_path = Path::new(&module_dir).join("__init__.py").sanitize();

    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &module_dir, &init_path);
    BuildScheduler::process_rebuilds(&mut session, false);

    (session, module_dir)
}

fn get_file(session: &SessionInfo, module_dir: &str, name: &str) -> SymbolKey {
    let syms = session
        .sync_odoo
        .get_symbol(module_dir, (&[name], &[]), u32::MAX);
    assert!(!syms.is_empty(), "expected file symbol for {}", name);
    syms[0]
}

#[test]
fn test_build_now_on_previous_step_is_noop() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").unwrap_buildable_key();

    // Sanity check on the fixture: a full build leaves file_a at VALIDATION/DONE, which
    // makes ARCH a "previous" step (collapsed to DONE, not tracked as its own PENDING slot).
    assert_eq!(session.st().get_current_build_step(file_a), BuildSteps::VALIDATION);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
    assert!(!session.st().ready_for_step(file_a, BuildSteps::ARCH));

    // Asking to (re)build the already-past ARCH step must not touch anything: build_now_impl
    // bails out on `ready_for_step` before it ever reaches the dependency walk or the
    // PythonArchBuilder call.
    BuildScheduler::build_now(&mut session, file_a, BuildSteps::ARCH);

    assert_eq!(session.st().get_current_build_step(file_a), BuildSteps::VALIDATION);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
}

#[test]
fn test_set_build_status_on_previous_step_to_done_is_noop() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").unwrap_buildable_key();
    assert_eq!(session.st().get_current_build_step(file_a), BuildSteps::VALIDATION);

    // Re-asserting DONE on an already-past step is allowed and changes nothing.
    session.st_mut().set_build_status(file_a, BuildSteps::ARCH, BuildStatus::DONE);

    assert_eq!(session.st().get_current_build_step(file_a), BuildSteps::VALIDATION);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
}

#[test]
#[should_panic(expected = "A previous build step should not be changed to a non-DONE status")]
fn test_set_build_status_on_previous_step_to_pending_panics() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").unwrap_buildable_key();

    // Rewinding a past step to a non-DONE status must go through `SymbolTable::invalidate`
    // instead, which also requeues dependents. `set_build_status` refuses it outright rather
    // than silently desyncing `current_build_step` from what already ran.
    session.st_mut().set_build_status(file_a, BuildSteps::ARCH, BuildStatus::PENDING);
}

#[test]
fn test_queue_is_noop_once_step_is_done() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").unwrap_buildable_key();
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(BuildScheduler::get_rebuild_queue_size(&mut session), 0);

    // `queue()` only inserts when the symbol's current step is PENDING; a fully-built symbol
    // has nothing left to schedule.
    BuildScheduler::queue(&mut session, file_a);

    assert_eq!(BuildScheduler::get_rebuild_queue_size(&mut session), 0);
}

#[test]
fn test_buildsteps_steps() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").unwrap_buildable_key();

    assert_eq!(session.st().get_current_build_step(file_a), BuildSteps::VALIDATION);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);

    //test by moving it manually
    SymbolTable::invalidate(&mut session, file_a.as_source_file_key().unwrap(), BuildSteps::ARCH);
    session.st_mut().set_build_status(file_a, BuildSteps::ARCH, BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);
    session.st_mut().set_build_status(file_a, BuildSteps::ARCH_EVAL, BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::PENDING);
    session.st_mut().set_build_status(file_a, BuildSteps::VALIDATION, BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);

    //now reset it and test it with build_now
    SymbolTable::invalidate(&mut session, file_a.as_source_file_key().unwrap(), BuildSteps::ARCH);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::PENDING);
    BuildScheduler::build_now(&mut session, file_a, BuildSteps::ARCH);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);
    BuildScheduler::build_now(&mut session, file_a, BuildSteps::ARCH_EVAL);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::PENDING);
    BuildScheduler::build_now(&mut session, file_a, BuildSteps::VALIDATION);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_a, BuildSteps::VALIDATION), BuildStatus::DONE);
}
