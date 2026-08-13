// Regression tests for a crash when resolving the class argument of super(cls, self):
// SymbolTable::follow_ref(...) can legitimately hand back a non-class sentinel (ANY, NONE,
// UNBOUND, DOMAIN...) instead of a resolvable class, or an empty Vec, or a reference that has
// since expired -- resolve_super_class_args (evaluation.rs) now scans every candidate
// evaluation and every follow_ref result instead of blindly indexing [0] and unwrapping,
// falling back to no super-class evaluation attached when nothing usable is found. These
// fixtures each used to panic before that fix; they must now build cleanly.
use std::env;

use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::*;

fn diagnostics_path(name: &str) -> String {
    env::current_dir()
        .unwrap()
        .join("tests/data/python/diagnostics")
        .join(name)
        .sanitize()
}

// ClassAny = get_class_stub() where get_class_stub() is a stub-only ("...") function, so its
// return evaluation is Evaluation::new_any() (ANY sentinel). follow_ref on `ClassAny` then
// terminates on that ANY entry instead of a class.
#[test]
fn test_super_first_arg_any_does_not_panic() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let path = diagnostics_path("panic_super_arg_any.py");
    prepare_custom_entry_point(&mut session, &path);
}

// ClassNone = get_class_no_return() where get_class_no_return() has a body but no return
// statement, so its return evaluation is Evaluation::new_none() (NONE sentinel) instead of
// new_any().
#[test]
fn test_super_first_arg_none_does_not_panic() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let path = diagnostics_path("panic_super_arg_none.py");
    prepare_custom_entry_point(&mut session, &path);
}

// Alias = ClassMaybe, where ClassMaybe is only conditionally assigned (`if int(): ClassMaybe
// = Base`, no else). Referencing "Alias" itself is unconditional so it evaluates to a single
// Evaluation, but Alias's own evaluations are [WEAK(ClassMaybe), UNBOUND("ClassMaybe")], and
// ClassMaybe needs one more hop through next_refs before it becomes terminal -- that hop
// re-queues it behind the zero-hop UNBOUND entry, so follow_ref's first result used to be
// UNBOUND, not the resolved class.
#[test]
fn test_super_first_arg_unbound_does_not_panic() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let path = diagnostics_path("panic_super_arg_unbound.py");
    prepare_custom_entry_point(&mut session, &path);
}
