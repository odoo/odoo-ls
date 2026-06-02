use std::env;
use std::path::PathBuf;
use std::rc::Rc;

use odoo_ls_server::constants::{BuildStatus, BuildSteps, OYarn};
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::Sy;
use odoo_ls_server::utils::PathSanitizer;

mod setup;
use setup::setup::{create_init_session, setup_server};

/// Regression test for RefCell borrow panic in build_now_dependencies.
///
/// The bug: build_now_dependencies held an immutable borrow on a symbol while
/// iterating its dependencies and calling build_now recursively. If the recursive
/// call chain reached eval_symbols_from_import_stmt, which tries to borrow_mut
/// the same symbol via add_dependency, it panicked with "RefCell already borrowed".
///
/// The fix: collect dependencies into an owned Vec, drop the borrow, then iterate.
///
/// Setup: file_b has class ClassB(file_c.ClassC), creating an ARCH_EVAL dep on
/// file_c. file_c imports file_b. After initial build, we manually set both files
/// to PENDING for ARCH_EVAL and call build_now(file_b) directly, bypassing
/// process_rebuilds' dependency-aware ordering.
///
/// Chain: build_now(file_b) → build_now_dependencies(file_b) borrows file_b →
/// finds file_c as ARCH_EVAL dep → build_now(file_c) → eval_arch(file_c) →
/// "from . import file_b" → add_dependency → file_b.borrow_mut() → PANIC.
#[test]
fn test_circular_dep_no_borrow_panic() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = PathBuf::from(odoo_path).sanitize();

    let file_b_syms = session.sync_odoo.get_symbol(
        &odoo_path,
        &(
            vec![
                Sy!("odoo"),
                Sy!("addons"),
                Sy!("module_borrow_test"),
                Sy!("models"),
                Sy!("file_b"),
            ],
            vec![],
        ),
        u32::MAX,
    );
    assert!(!file_b_syms.is_empty(), "file_b symbol not found");
    let file_b = file_b_syms[0].clone();

    let file_c_syms = session.sync_odoo.get_symbol(
        &odoo_path,
        &(
            vec![
                Sy!("odoo"),
                Sy!("addons"),
                Sy!("module_borrow_test"),
                Sy!("models"),
                Sy!("file_c"),
            ],
            vec![],
        ),
        u32::MAX,
    );
    assert!(!file_c_syms.is_empty(), "file_c symbol not found");
    let file_c = file_c_syms[0].clone();

    assert_eq!(
        file_b.borrow().build_status(BuildSteps::ARCH_EVAL),
        BuildStatus::DONE,
        "file_b should be DONE after initial build"
    );
    assert_eq!(
        file_c.borrow().build_status(BuildSteps::ARCH_EVAL),
        BuildStatus::DONE,
        "file_c should be DONE after initial build"
    );

    // Verify the key dependency: file_b has an ARCH_EVAL dep on file_c
    // (from ClassB inheriting file_c.ClassC).
    let has_dep = {
        let sym = file_b.borrow();
        let deps = sym.get_all_dependencies(BuildSteps::ARCH_EVAL);
        deps.map_or(false, |all_dep| {
            all_dep
                .get(BuildSteps::ARCH_EVAL as usize)
                .and_then(|d| d.as_ref())
                .map_or(false, |dep_set| dep_set.iter().any(|d| Rc::ptr_eq(&d, &file_c)))
        })
    };
    assert!(
        has_dep,
        "file_b should have an ARCH_EVAL dep on file_c (from class inheritance)"
    );

    // Set both files to PENDING for ARCH_EVAL, simulating invalidation.
    file_b
        .borrow_mut()
        .set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
    file_c
        .borrow_mut()
        .set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);

    // Directly call build_now on file_b. This bypasses process_rebuilds' smart
    // ordering (which would process file_c first to avoid the conflict).
    //
    // With old code: build_now_dependencies(file_b) holds borrow on file_b,
    // then calls build_now(file_c) → eval_arch(file_c) → imports file_b →
    // add_dependency tries file_b.borrow_mut() → PANIC.
    //
    // With fix: deps are collected into Vec, borrow released, then iterated.
    SyncOdoo::build_now(&mut session, &file_b, BuildSteps::ARCH_EVAL);
}
