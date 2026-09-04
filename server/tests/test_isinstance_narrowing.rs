mod setup;
mod test_utils;

use lsp_types::{
    Location, PartialResultParams, Position, ReferenceContext, ReferenceParams,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::env;
use std::path::Path;
use test_utils::get_resolved_symbols_at_position;

fn narrowing_fixture_path() -> String {
    env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/isinstance_narrowing.py")
        .sanitize()
}

/// Spec / triage suite for `isinstance()`-based type narrowing. None of these are
/// implemented yet (no `isinstance` narrowing exists in the evaluator); this test
/// documents the intended behavior per case so failures can be picked apart and
/// prioritized individually rather than implemented all-or-nothing.
#[test]
fn test_isinstance_narrowing() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = narrowing_fixture_path();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    let animal = session.st().get_sub_symbol(file_symbol.into(), "Animal", u32::MAX).symbols[0];
    let dog = session.st().get_sub_symbol(file_symbol.into(), "Dog", u32::MAX).symbols[0];
    let cat = session.st().get_sub_symbol(file_symbol.into(), "Cat", u32::MAX).symbols[0];
    let other = session.st().get_sub_symbol(file_symbol.into(), "Other", u32::MAX).symbols[0];

    // (case name, (line, character), expected resolved types, order-independent)
    let cases = [
        // Plain positive narrowing inside the `if` body.
        ("basic_if", (26, 8), vec![dog]),
        // Narrowing is implemented as a synthetic reassignment scoped to the `if` body, so it
        // inherits this codebase's existing behavior for any conditional reassignment with no
        // `else`: the type widens to a union of "branch taken" and "branch not taken" after the
        // block (see tests/data/python/expressions/follow_ref.py's `# b: (int | TestClass)`).
        ("narrowing_ends_after_if", (32, 4), vec![animal, dog]),
        // `else` of a positive check must NOT be narrowed to the checked subtype.
        ("else_branch_not_narrowed_to_subtype", (39, 8), vec![animal]),
        // if/elif/else chain: each branch narrows to its own check, else stays unnarrowed.
        ("elif_chain: if-branch", (44, 8), vec![dog]),
        ("elif_chain: elif-branch", (46, 8), vec![cat]),
        ("elif_chain: else-branch", (48, 8), vec![animal]),
        // `if not isinstance(x, T): return` guard narrows the fall-through code to T.
        ("negative_guard_return", (54, 4), vec![dog]),
        // The guard's guarantee only depends on *its own* test having been false to reach the
        // fallthrough - unrelated earlier conditions in the same if/elif chain don't weaken it.
        ("negative_guard_after_unrelated_conditions", (66, 4), vec![dog]),
        // A branch that does *not* exit joins the merge unnarrowed alongside the narrowed
        // fallthrough - the guarantee only holds when every other branch is excluded.
        ("negative_guard_with_non_exiting_branch", (77, 4), vec![animal, dog]),
        // The negated isinstance check is the *first* test, not the last - narrowing must
        // propagate forward through the elif's own test section, not just the final fallthrough.
        ("negative_guard_not_the_last_condition", (88, 4), vec![dog]),
        // `assert isinstance(x, T)` narrows the rest of the block to T.
        ("assert_narrows", (93, 4), vec![dog]),
        // `isinstance(x, (A, B))` narrows to the union of A and B.
        ("tuple_of_types", (98, 8), vec![dog, cat]),
        // Narrowing from the left operand of `and` applies to the right operand and body.
        ("and_combined_condition", (103, 8), vec![dog]),
        // Reassigning inside the narrowed block must drop the narrowing.
        ("reassignment_invalidates_narrowing", (109, 8), vec![animal]),
        // Narrowing composes through nested `if isinstance(...)` checks.
        ("nested_isinstance", (115, 12), vec![dog]),
        // Narrowing also applies to attribute accesses (`self.animal`), not just locals.
        ("narrows_attribute (self.animal)", (125, 12), vec![dog]),
        // `while isinstance(x, T):` narrows the loop body to T.
        ("while_condition", (130, 8), vec![dog]),
        // Pre-existing imprecision (see visit_while's "TODO: Handle breaks for sections"): the
        // merge also includes the body's own end-state as if finishing one iteration were an
        // exit, so a reassigning body widens this to a union instead of a clean Dog.
        ("while_negative_guard", (136, 4), vec![animal, dog]),
        // The loop's `else` clause also only runs on a normal exit, so it gets the same
        // narrowing as the code after the whole statement.
        ("while_negative_guard_with_else", (143, 8), vec![dog]),
        // Narrowing applies inside the true-branch of a conditional expression too.
        ("ternary_expression", (147, 8), vec![dog]),
        // isinstance against an unrelated (non-hierarchy) class still narrows to it.
        ("unrelated_type_check", (153, 8), vec![other]),
    ];

    // Run every case and print a full triage report before failing, so all results
    // are visible in one pass instead of stopping at the first mismatch.
    let case_count = cases.len();
    let mut failures = Vec::new();
    for (name, (line, character), expected) in cases {
        let resolved = get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, line, character);
        let mut resolved_names: Vec<String> = resolved.iter().map(|&s| session.st().name(s).to_string()).collect();
        let mut expected_names: Vec<String> = expected.iter().map(|&s| session.st().name(s).to_string()).collect();
        resolved_names.sort();
        expected_names.sort();
        if resolved_names != expected_names {
            failures.push(name);
        }
    }

    assert!(failures.is_empty(), "{} of {} case(s) not implemented: {:?}", failures.len(), case_count, failures);
}

/// The synthetic narrowing re-declaration must be transparent to go-to-definition: clicking on
/// a narrowed read should land on the real declaration (here, the `animal` parameter), not on
/// the synthetic node's own made-up position.
#[test]
fn test_narrowing_preserves_go_to_definition() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = narrowing_fixture_path();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");

    // Line 26 (0-indexed): the narrowed `animal` read inside `basic_if`'s `if` body.
    let locations = test_utils::get_definition_locs(&mut session, file_symbol, &file_info, 26, 8);
    assert_eq!(locations.len(), 1, "expected exactly one definition, got {locations:?}");
    let range = locations[0].target_range;
    // The `animal` parameter of `basic_if` (line 25, 0-indexed) - its declared range spans the
    // whole `animal: Animal`, columns 13..27.
    assert_eq!(
        (range.start.line, range.start.character, range.end.line, range.end.character),
        (24, 13, 24, 27),
        "go-to-definition on a narrowed read should resolve to the `animal` parameter, not the synthetic narrowing node"
    );
}

/// Symmetric with go-to-definition: "find references" seeded from the real declaration must
/// still find narrowed reads inside the block they're scoped to.
#[test]
fn test_narrowing_preserves_find_references() {
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = narrowing_fixture_path();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());

    // Position(24, 13): the `animal` parameter of `basic_if`.
    let references = get_references(&mut session, &path, Position::new(24, 13));
    let narrowed_read_found = references.iter().any(|r| {
        r.uri.as_str().ends_with("isinstance_narrowing.py")
            && r.range.start.line == 26
            && r.range.start.character == 8
    });
    assert!(
        narrowed_read_found,
        "find-references from the `animal` parameter should include the narrowed read at line 27, got: {:?}",
        references.iter().map(|r| (r.range.start.line, r.range.start.character)).collect::<Vec<_>>()
    );
}

fn get_references(session: &mut SessionInfo, path: &str, position: Position) -> Vec<Location> {
    let references_params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: FileMgr::pathname2uri(path) },
            position,
        },
        context: ReferenceContext { include_declaration: true },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    Odoo::handle_references(session, references_params)
        .expect("handle_references returned Err")
        .expect("handle_references returned None")
}
