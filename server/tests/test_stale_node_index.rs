use std::env;
use std::path::PathBuf;

use lsp_types::TextDocumentContentChangeEvent;
use odoo_ls_server::core::entry_point::EntryPointMgr;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::storage::SymbolTable;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

// `stale_file.py` re-parsed with content holding no function at all, so every node index
// the old parse handed out now addresses something that is not a `Stmt::FunctionDef`.
// The statement count is padding: indexes are dense and an out-of-range one would panic
// inside `IndexedModule::get_by_index` instead of on the type mismatch this targets.
const REPARSED_WITHOUT_FUNCTION: &str = "\
first = 1
second = 2
third = 3
fourth = 4
fifth = 5
sixth = 6
seventh = 7
eighth = 8
";

// Regression test for the panic at python_arch_eval.rs:112, reproducing the sequence from
// the crash report: a semantic tokens request resolves a name, which calls
// `ensure_func_evaluations` on a method whose file has been re-parsed since ARCH walked
// it. The method still holds the `node_index` of the parse that is gone, so the function
// branch of `eval_arch` handed a stale index to `get_by_index` and hit
// `panic!("Expected function definition")`.
//
// Only methods take that branch: `PythonArchBuilder::visit_func_def` walks a module-level
// function's body inline during the file's ARCH, and defers only class members to a
// later function-mode build.
#[test]
fn test_stale_node_index_on_method_does_not_panic() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let module_dir = env::current_dir()
        .unwrap()
        .join("tests/data/addons/stale_node_index_module")
        .sanitize();
    let init_path = PathBuf::from(&module_dir).join("__init__.py").sanitize();
    let file_path = PathBuf::from(&module_dir).join("stale_file.py").sanitize();

    EntryPointMgr::create_new_custom_entry_for_path(&mut session, &module_dir, &init_path);
    SyncOdoo::process_rebuilds(&mut session, false);

    let file = session
        .sync_odoo
        .get_symbol(&module_dir, (&["stale_file"], &[]), u32::MAX)
        .first()
        .expect("expected a file symbol for stale_file")
        .as_source_file_key()
        .expect("stale_file should be a source file");
    let method = session
        .sync_odoo
        .get_symbol(&module_dir, (&["stale_file"], &["Target", "target_method"]), u32::MAX)
        .first()
        .expect("expected a function symbol for target_method")
        .unwrap_function_key();

    // Build the method the way a feature request does, leaving it ARCH and ARCH_EVAL done
    // with a node_index into the current parse.
    SyncOdoo::ensure_func_evaluations(&mut session, method);

    // Refresh only the AST. Nothing unloads the symbols, so `processed_text_hash` keeps its
    // old value and the method keeps its old `node_index` — the window this hits in
    // production between a disk change and the deferred rebuild.
    let change = [TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: REPARSED_WITHOUT_FUNCTION.to_string(),
    }];
    let (updated, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(
        &mut session,
        &file_path,
        Some(change.as_slice()),
        Some(1),
        false,
    );
    assert!(updated, "the file should have been re-parsed");
    assert_ne!(
        file_info.borrow().file_info_ast.borrow().text_hash,
        session.st().get_processed_text_hash(file),
        "the re-parse should have left the file symbol's hash stale"
    );

    // Invalidation through a changed dependency: evaluations cleared and ARCH_EVAL back to
    // PENDING, while ARCH stays DONE. That is the state that makes the next call take the
    // function branch of eval_arch.
    SymbolTable::invalidate_sub_functions(&mut session, file);

    // Without the guard, this panics with "Expected function definition".
    SyncOdoo::ensure_func_evaluations(&mut session, method);
}
