use std::env;
use std::path::Path;

use odoo_ls_server::constants::{BuildStatus, BuildSteps};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::config::ConfigEntry;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::{BuildableSymbolKey, SourceFileKey, SymbolKey};
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

fn get_method_a(session: &SessionInfo, file_a: SourceFileKey) -> BuildableSymbolKey {
    let methods = session.st().get_symbol(SymbolKey::from(file_a), (&[], &["ClassA", "method_a"]), u32::MAX);
    assert_eq!(methods.len(), 1, "expected ClassA.method_a");
    methods[0].unwrap_buildable_key()
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

/// Invalidating a file, whatever the step, sends its methods back to ARCH_EVAL: they are
/// evaluated again by the function queues before the validation of the file.
#[test]
fn test_invalidate_resets_methods() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").as_source_file_key().unwrap();
    let methods = session.st().get_symbol(SymbolKey::from(file_a), (&[], &["ClassA", "method_a"]), u32::MAX);
    assert_eq!(methods.len(), 1, "expected ClassA.method_a");
    let method = methods[0].unwrap_buildable_key();
    let assert_method = |session: &SessionInfo, arch_eval: BuildStatus, validation: BuildStatus| {
        assert_eq!(session.st().build_status(method, BuildSteps::ARCH_EVAL), arch_eval);
        assert_eq!(session.st().build_status(method, BuildSteps::VALIDATION), validation);
    };
    assert_method(&session, BuildStatus::DONE, BuildStatus::DONE);

    for step in [BuildSteps::ARCH_EVAL, BuildSteps::VALIDATION] {
        SymbolTable::invalidate(&mut session, file_a, step);
        assert_method(&session, BuildStatus::PENDING, BuildStatus::PENDING);
        // without validation, the methods are still built (requests need them), but not validated
        BuildScheduler::process_rebuilds(&mut session, true);
        assert_method(&session, BuildStatus::DONE, BuildStatus::PENDING);
        BuildScheduler::process_rebuilds(&mut session, false);
        assert_method(&session, BuildStatus::DONE, BuildStatus::DONE);
    }
}

/// While something is being built, an invalidation is only requested, as the build in progress
/// may still use what it would remove. It is applied at the end of the build phase, and queues the
/// symbols to build again.
#[test]
fn test_invalidate_is_deferred_while_building() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").as_source_file_key().unwrap();
    let method = get_method_a(&session, file_a);

    BuildScheduler::defer_invalidations(&mut session, |session| {
        SymbolTable::invalidate(session, file_a, BuildSteps::ARCH_EVAL);
        assert_eq!(session.st().build_status(file_a.into(), BuildSteps::ARCH_EVAL), BuildStatus::DONE);
        assert_eq!(session.st().build_status(method, BuildSteps::ARCH_EVAL), BuildStatus::DONE);
    });
    // the invalidation waits for the end of the build phase
    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::ARCH_EVAL), BuildStatus::DONE);

    SymbolTable::apply_pending_invalidations(&mut session);
    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::ARCH_EVAL), BuildStatus::PENDING);
    assert_eq!(session.st().build_status(method, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);

    BuildScheduler::process_rebuilds(&mut session, false);
    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(session.st().build_status(method, BuildSteps::VALIDATION), BuildStatus::DONE);
}

/// The invalidation of a file being built is overtaken by its build, that sets its step DONE at
/// its end: only its methods, that are not built with it, are invalidated. As its validation used
/// them, it is validated again with them.
#[test]
fn test_invalidate_file_being_built() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").as_source_file_key().unwrap();
    let method = get_method_a(&session, file_a);
    // file_a waits for its validation, with its methods built
    SymbolTable::invalidate(&mut session, file_a, BuildSteps::VALIDATION);
    BuildScheduler::process_rebuilds(&mut session, true);
    assert_eq!(session.st().build_status(method, BuildSteps::ARCH_EVAL), BuildStatus::DONE);

    BuildScheduler::defer_invalidations(&mut session, |session| {
        // invalidated during its own validation
        session.st_mut().set_build_status(file_a.into(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        SymbolTable::invalidate(session, file_a, BuildSteps::VALIDATION);
        session.st_mut().set_build_status(file_a.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
    });
    SymbolTable::apply_pending_invalidations(&mut session);

    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::VALIDATION), BuildStatus::PENDING);
    assert_eq!(session.st().build_status(method, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);

    BuildScheduler::process_rebuilds(&mut session, false);
    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(session.st().build_status(method, BuildSteps::VALIDATION), BuildStatus::DONE);
}

/// The dependents of a file invalidated while something is being built are read once the file is reset,
/// not when the invalidation is requested: until the reset, a dependent can still be built with what the
/// file built before, and has to be built again too.
#[test]
fn test_dependent_built_before_the_reset_is_invalidated() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").as_source_file_key().unwrap();
    // file_c inherits ClassA: its ARCH_EVAL depends on the one of file_a
    let file_c = get_file(&session, &module_dir, "file_c").as_source_file_key().unwrap();
    assert_eq!(session.st().build_status(file_c.into(), BuildSteps::VALIDATION), BuildStatus::DONE);

    // file_c waits for its ARCH_EVAL, in the same phase as the build invalidating file_a
    SymbolTable::invalidate(&mut session, file_c, BuildSteps::ARCH_EVAL);
    BuildScheduler::defer_invalidations(&mut session, |session| {
        SymbolTable::invalidate(session, file_a, BuildSteps::ARCH_EVAL);
        // file_c is built before file_a is reset, with what file_a built before
        session.st_mut().set_build_status(file_c.into(), BuildSteps::ARCH_EVAL, BuildStatus::IN_PROGRESS);
        session.st_mut().set_build_status(file_c.into(), BuildSteps::ARCH_EVAL, BuildStatus::DONE);
    });
    SymbolTable::apply_pending_invalidations(&mut session);

    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::ARCH_EVAL), BuildStatus::PENDING);
    assert_eq!(session.st().build_status(file_c.into(), BuildSteps::ARCH_EVAL), BuildStatus::PENDING,
        "file_c was built with the previous ARCH_EVAL of file_a");

    BuildScheduler::process_rebuilds(&mut session, false);
    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::VALIDATION), BuildStatus::DONE);
    assert_eq!(session.st().build_status(file_c.into(), BuildSteps::VALIDATION), BuildStatus::DONE);
}

fn arch_eval_status(session: &SessionInfo, file: SourceFileKey) -> BuildStatus {
    session.st().build_status(file.into(), BuildSteps::ARCH_EVAL)
}

/// Build module_buildsteps, and reset the ARCH_EVAL of file_a, and of file_c with it (it inherits ClassA): both
/// wait in the ARCH_EVAL queue, file_a first. Returns them.
fn queue_arch_eval_of_a_and_c(session: &mut SessionInfo, module_dir: &str) -> (SourceFileKey, SourceFileKey) {
    let file_a = get_file(session, module_dir, "file_a").as_source_file_key().unwrap();
    let file_c = get_file(session, module_dir, "file_c").as_source_file_key().unwrap();
    SymbolTable::invalidate(session, file_a, BuildSteps::ARCH_EVAL);
    assert_eq!(arch_eval_status(session, file_a), BuildStatus::PENDING);
    assert_eq!(arch_eval_status(session, file_c), BuildStatus::PENDING);
    (file_a, file_c)
}

/// An invalidation requested during a build phase is applied once the phase is over, not before the next
/// build of the phase: a file already built in the phase is then built again, once.
#[test]
fn test_invalidation_applied_at_the_end_of_the_phase() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);
    let (file_a, file_c) = queue_arch_eval_of_a_and_c(&mut session, &module_dir);
    let entry = session.st().get_entry(file_a);

    assert!(BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_a), BuildStatus::DONE);
    assert_eq!(arch_eval_status(&session, file_c), BuildStatus::PENDING);

    // a build of the phase invalidates file_a, already built
    BuildScheduler::defer_invalidations(&mut session, |session| {
        SymbolTable::invalidate(session, file_a, BuildSteps::ARCH_EVAL);
    });

    // file_c is still in the ARCH_EVAL phase: the invalidation waits
    assert!(BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_a), BuildStatus::DONE);
    assert_eq!(arch_eval_status(&session, file_c), BuildStatus::DONE);

    // the phase is over: file_a is reset, and file_c with it as it was built with the previous ARCH_EVAL of
    // file_a. They are queued again in that order, and file_a is built.
    assert!(BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_a), BuildStatus::DONE);
    assert_eq!(arch_eval_status(&session, file_c), BuildStatus::PENDING);

    assert!(BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_c), BuildStatus::DONE);
    // built once again each: nothing left
    assert!(!BuildScheduler::build_one(&mut session, entry, false));
}

/// The invalidation of a file that did not build the step yet is overtaken: it builds it with what
/// changed, and is not built again at the end of the phase.
#[test]
fn test_invalidation_of_a_file_not_built_yet_is_overtaken() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);
    let (file_a, file_c) = queue_arch_eval_of_a_and_c(&mut session, &module_dir);
    let entry = session.st().get_entry(file_a);

    assert!(BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_a), BuildStatus::DONE);

    // a build of the phase invalidates file_c, still waiting for its ARCH_EVAL
    BuildScheduler::defer_invalidations(&mut session, |session| {
        SymbolTable::invalidate(session, file_c, BuildSteps::ARCH_EVAL);
    });

    assert!(BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_c), BuildStatus::DONE);
    // the phase is over, with nothing to build again
    assert!(!BuildScheduler::build_one(&mut session, entry, false));
    assert_eq!(arch_eval_status(&session, file_a), BuildStatus::DONE);
    assert_eq!(arch_eval_status(&session, file_c), BuildStatus::DONE);
}

/// The invalidation of an earlier step than the one a file is building is not overtaken by its build:
/// it is applied at the end of the build, and the file builds that step again.
#[test]
fn test_invalidate_earlier_step_of_file_being_built() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let (mut session, module_dir) = build_module(&mut odoo, config);

    let file_a = get_file(&session, &module_dir, "file_a").as_source_file_key().unwrap();
    // file_a waits for its validation
    SymbolTable::invalidate(&mut session, file_a, BuildSteps::VALIDATION);

    BuildScheduler::defer_invalidations(&mut session, |session| {
        // its ARCH_EVAL is invalidated during its validation
        session.st_mut().set_build_status(file_a.into(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        SymbolTable::invalidate(session, file_a, BuildSteps::ARCH_EVAL);
        session.st_mut().set_build_status(file_a.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
    });
    SymbolTable::apply_pending_invalidations(&mut session);
    assert_eq!(arch_eval_status(&session, file_a), BuildStatus::PENDING);

    BuildScheduler::process_rebuilds(&mut session, false);
    assert_eq!(session.st().build_status(file_a.into(), BuildSteps::VALIDATION), BuildStatus::DONE);
}
