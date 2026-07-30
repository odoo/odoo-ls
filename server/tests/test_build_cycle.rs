use std::env;
use std::path::Path;

use odoo_ls_server::constants::{BuildStatus, BuildSteps};
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::SymbolKey;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

// Regression test for a stack overflow in SyncOdoo::build_now when three files form
// an ARCH_EVAL dep cycle via mutual class-base references. The cycle guard in
// build_now_impl short-circuits re-entry on the same (symbol, step); without it,
// build_now ↔ build_now_dependencies recurse until the stack is exhausted.
#[test]
fn test_build_now_arch_eval_cycle_does_not_overflow() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let module_dir = env::current_dir()
        .unwrap()
        .join("tests/data/addons/build_cycle_module")
        .sanitize();
    let init_path = Path::new(&module_dir).join("__init__.py").sanitize();

    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &module_dir, &init_path);
    BuildScheduler::process_rebuilds(&mut session, false);

    // Grab the three file symbols. Tree lookup is relative to the entry point's
    // absolute-path tree prefix, so we just ask for the bare file name.
    let file_a = get_file(&session, &module_dir, "file_a");
    let file_b = get_file(&session, &module_dir, "file_b");
    let file_c = get_file(&session, &module_dir, "file_c");

    // Force all three files back into PENDING simultaneously. This simulates
    // invalation after a common dependency was changed. The class-base
    // ARCH_EVAL deps registered during the initial pass remain in the dep graph,
    // so build_now_dependencies walks file_a → file_b → file_c → file_a.
    BuildScheduler::queue(&mut session, file_a, BuildSteps::ARCH_EVAL);
    BuildScheduler::queue(&mut session, file_b, BuildSteps::ARCH_EVAL);
    BuildScheduler::queue(&mut session, file_c, BuildSteps::ARCH_EVAL);

    assert_eq!(session.st().build_status(file_a, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);
    assert_eq!(session.st().build_status(file_b, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);
    assert_eq!(session.st().build_status(file_c, BuildSteps::ARCH_EVAL), BuildStatus::PENDING);

    // Without the cycle guard in build_now_impl, this call stack-overflows.
    SyncOdoo::build_now(&mut session, file_a, BuildSteps::ARCH_EVAL);
}

fn get_file(session: &SessionInfo, module_dir: &str, name: &str) -> SymbolKey {
    let syms = session
        .sync_odoo
        .get_symbol(module_dir, (&[name], &[]), u32::MAX);
    assert!(!syms.is_empty(), "expected file symbol for {}", name);
    syms[0]
}
