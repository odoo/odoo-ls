mod setup;
mod test_utils;

use lsp_types::{
    CompletionResponse, Location, PartialResultParams, Position, ReferenceContext, ReferenceParams,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};
use odoo_ls_server::core::file_mgr::{FileInfo, FileMgr};
use odoo_ls_server::core::odoo::{Odoo, SyncOdoo};
use odoo_ls_server::core::symbols::symbol_keys::SourceFileKey;
use odoo_ls_server::features::completion::CompletionFeature;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use std::cell::RefCell;
use std::env;
use std::path::Path;
use std::rc::Rc;
use test_utils::{get_hover_markdown, get_resolved_symbols_at_position};

fn narrowing_fixture_path() -> String {
    env::current_dir()
        .unwrap()
        .join("tests/data/python/expressions/isinstance_narrowing.py")
        .sanitize()
}

/// Runs `f` against the shared narrowing fixture.
fn with_fixture<F>(f: F)
where
    F: FnOnce(&mut SessionInfo, &Rc<RefCell<FileInfo>>, SourceFileKey),
{
    let (mut odoo, config) = setup::setup::setup_server(false);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let path = narrowing_fixture_path();
    setup::setup::prepare_custom_entry_point(&mut session, path.as_str());
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&path).unwrap();
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&path))
        .expect("Failed to get file symbol");
    f(&mut session, &file_info, file_symbol);
}

fn completion_labels(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Vec<String> {
    match CompletionFeature::autocomplete(session, file_symbol, file_info, None, line, character) {
        Some(CompletionResponse::Array(items)) => items.iter().map(|i| i.label.clone()).collect(),
        Some(CompletionResponse::List(list)) => list.items.iter().map(|i| i.label.clone()).collect(),
        None => vec![],
    }
}

fn sorted_resolved_names(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Vec<String> {
    let mut names: Vec<String> = get_resolved_symbols_at_position(session, file_symbol, file_info, line, character)
        .iter()
        .map(|&s| session.st().name(s).to_string())
        .collect();
    names.sort();
    names
}

/// Spec suite for `isinstance()`-based type narrowing. Two known-unimplemented cases
/// (attribute narrowing, ternary narrowing) live in their own `#[ignore]`d tests below
/// instead of here, so this suite stays green and a real regression isn't lost in noise.
#[test]
fn test_isinstance_narrowing() {
    with_fixture(|session, file_info, file_symbol| {
        let animal = session.st().get_sub_symbol(file_symbol.into(), "Animal", u32::MAX).symbols[0];
        let dog = session.st().get_sub_symbol(file_symbol.into(), "Dog", u32::MAX).symbols[0];
        let cat = session.st().get_sub_symbol(file_symbol.into(), "Cat", u32::MAX).symbols[0];
        let other = session.st().get_sub_symbol(file_symbol.into(), "Other", u32::MAX).symbols[0];

        // (case name, (line, character), expected resolved types, order-independent)
        let cases = [
            // Plain positive narrowing inside the `if` body.
            ("basic_if", (26, 8), vec![dog]),
            // A synthetic reassignment scoped to the `if` body, so it inherits the existing
            // behavior of any conditional reassignment with no `else`: after the block the type
            // widens to a union (see follow_ref.py's `# b: (int | TestClass)`).
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
            // `while isinstance(x, T):` narrows the loop body to T.
            ("while_condition", (130, 8), vec![dog]),
            // Pre-existing imprecision (visit_while's "TODO: Handle breaks for sections"): the
            // post-loop merge includes the body's own end-state, where the test had not yet
            // failed, so the guaranteed type comes back unioned with the unnarrowed one.
            ("while_negative_guard", (136, 4), vec![animal, dog]),
            // The `else` clause runs only on a normal exit, and chains straight off the
            // loop-exit narrowing - so unlike the code after the loop, it is not widened.
            ("while_negative_guard_with_else", (143, 8), vec![dog]),
            // The narrowing has to be visible from a read whose own end offset is the earliest
            // possible query position - a one-character name.
            ("single_char_name", (169, 8), vec![dog]),
            // No separator after the `:` - the narrowing anchor lands on the colon itself.
            ("one_liner_no_space", (199, 31), vec![dog]),
            ("one_liner_semicolons: 1st", (203, 31), vec![dog]),
            ("one_liner_semicolons: 2nd", (203, 38), vec![dog]),
            ("one_liner_boolop_no_space", (207, 40), vec![dog]),
            ("one_liner_while", (211, 34), vec![dog]),
            // `and`-chain whose *last* operand is the check - the body only runs if it was true.
            ("and_chain_check_last", (216, 8), vec![dog]),
            // An `elif`'s test is visited like the main `if`'s: an `and`-chain's earlier operands
            // (the check, and a plain walrus) must be visible in its own body.
            ("elif_and_chain_walrus: `animal`", (223, 8), vec![dog]),
            ("elif_and_chain_walrus: `z`", (224, 8), vec![animal]),
            // isinstance against an unrelated (non-hierarchy) class still narrows to it.
            ("unrelated_type_check", (153, 8), vec![other]),
            // Unresolvable checked type: narrowing must be a no-op, not erase the original type.
            ("unresolvable_type_name", (235, 8), vec![animal]),
            // A `while` test that is an `and`-chain still narrows its body.
            ("while_and_chain", (240, 8), vec![dog]),
            // `;` right after the assert puts the next statement on the narrowing's own anchor.
            ("one_liner_assert", (244, 35), vec![dog]),
            // The narrowing shares its position with the reassignment it shadows; the reassigned
            // type must win, i.e. the two must stay distinguishable at that one position.
            ("narrowing_collides_with_reassignment", (258, 8), vec![cat]),
            // A false `or` means every operand failed, so a negated check in one still guarantees
            // its type afterwards - the De Morgan dual of the `and` rule.
            ("or_negative_guard", (264, 4), vec![dog]),
            // Same rule one level down: the second operand is only reached once the first failed.
            ("or_operand_sees_previous_negation", (268, 38), vec![dog]),
            // A false `or` of *positive* checks says only what the value is not: nothing to do.
            ("or_of_positive_checks_else", (276, 8), vec![animal]),
            // A true `or` of checks on one name narrows to the union of what each allows.
            ("or_of_positive_checks", (281, 8), vec![dog, cat]),
            // ... but only if *every* operand narrows that name - `flag` could be what held.
            ("or_with_non_check_operand", (286, 8), vec![animal]),
            // Same reason across names: neither is guaranteed by the whole `or`.
            ("or_on_different_names", (291, 8), vec![animal]),
            // `else` is clean, but the code after the same loop is not: see while_negative_guard.
            ("while_else_then_after: in else", (305, 8), vec![dog]),
            ("while_else_then_after: after", (306, 4), vec![animal, dog]),
            // A loop's `else` runs after zero or more iterations, so it must see what the body
            // bound. Without this it fell through to the module-level `found` decoy.
            ("while_else_sees_body_binding: `found`", (313, 8), vec![animal]),
            // ... while the narrowed name still gets the clean type: the narrowing re-declares
            // it, shadowing the body's own binding.
            ("while_else_sees_body_binding: `animal`", (314, 8), vec![dog]),
            ("for_else_sees_body_binding: `found`", (321, 8), vec![animal]),
            // The mirror case: a false `and` of negated checks means at least one of them held.
            ("and_of_negated_checks_else", (298, 8), vec![dog, cat]),
        ];

        // Collect every mismatch before failing, so one run reports all of them.
        let case_count = cases.len();
        let mut failures = Vec::new();
        for (name, (line, character), expected) in cases {
            let resolved = get_resolved_symbols_at_position(session, file_symbol, file_info, line, character);
            let mut resolved_names: Vec<String> = resolved.iter().map(|&s| session.st().name(s).to_string()).collect();
            let mut expected_names: Vec<String> = expected.iter().map(|&s| session.st().name(s).to_string()).collect();
            resolved_names.sort();
            expected_names.sort();
            if resolved_names != expected_names {
                failures.push(format!("{name}: expected {expected_names:?}, got {resolved_names:?}"));
            }
        }

        assert!(failures.is_empty(), "{} of {} case(s) not implemented: {:?}", failures.len(), case_count, failures);
    });
}

#[test]
#[ignore = "attribute narrowing (self.x) not implemented - needs attribute type-hint resolution first"]
fn test_narrows_attribute() {
    with_fixture(|session, file_info, file_symbol| {
        let dog = session.st().get_sub_symbol(file_symbol.into(), "Dog", u32::MAX).symbols[0];

        let resolved = get_resolved_symbols_at_position(session, file_symbol, file_info, 125, 12);
        let resolved_names: Vec<String> = resolved.iter().map(|&s| session.st().name(s).to_string()).collect();
        assert_eq!(resolved_names, vec![session.st().name(dog).to_string()]);
    });
}

/// `assert isinstance(x, T);stmt` puts the next statement on the narrowing's own anchor, so this
/// only works because completion queries from the end of the attribute's value, not its start.
#[test]
fn test_completion_after_one_liner_assert() {
    with_fixture(|session, file_info, file_symbol| {
        let labels = completion_labels(session, file_symbol, file_info, 248, 42);
        assert!(labels.contains(&"bark".to_string()), "expected Dog's `bark`, got {labels:?}");
    });
}

#[test]
#[ignore = "ternary narrowing not implemented - Expr::If is a no-op in both ARCH passes"]
fn test_ternary_expression() {
    with_fixture(|session, file_info, file_symbol| {
        let dog = session.st().get_sub_symbol(file_symbol.into(), "Dog", u32::MAX).symbols[0];

        let resolved = get_resolved_symbols_at_position(session, file_symbol, file_info, 147, 8);
        let resolved_names: Vec<String> = resolved.iter().map(|&s| session.st().name(s).to_string()).collect();
        assert_eq!(resolved_names, vec![session.st().name(dog).to_string()]);
    });
}

/// The synthetic re-declaration must be transparent to go-to-definition: a narrowed read lands
/// on the real declaration (the `animal` parameter), not on the synthetic node's made-up position.
#[test]
fn test_narrowing_preserves_go_to_definition() {
    with_fixture(|session, file_info, file_symbol| {
        // Line 26 (0-indexed): the narrowed `animal` read inside `basic_if`'s `if` body.
        let locations = test_utils::get_definition_locs(session, file_symbol, file_info, 26, 8);
        assert_eq!(locations.len(), 1, "expected exactly one definition, got {locations:?}");
        let range = locations[0].target_range;
        // The `animal` parameter of `basic_if` (line 25, 0-indexed) - its declared range spans the
        // whole `animal: Animal`, columns 13..27.
        assert_eq!(
            (range.start.line, range.start.character, range.end.line, range.end.character),
            (24, 13, 24, 27),
            "go-to-definition on a narrowed read should resolve to the `animal` parameter, not the synthetic narrowing node"
        );
    });
}

/// Symmetric with go-to-definition: "find references" seeded from the real declaration must
/// still find narrowed reads inside the block they're scoped to.
#[test]
fn test_narrowing_preserves_find_references() {
    with_fixture(|session, _file_info, _file_symbol| {
        let path = narrowing_fixture_path();
        // Position(24, 13): the `animal` parameter of `basic_if`.
        let references = get_references(session, &path, Position::new(24, 13));
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
    });
}

/// Completion and validation pass the statement's own start as `max_infer`, so the narrowing has
/// to be visible from the very first statement of the body - the most common way it is used.
#[test]
fn narrowing_applies_on_first_body_statement() {
    with_fixture(|session, file_info, file_symbol| {
        // Control: the same access one statement later.
        let second = completion_labels(session, file_symbol, file_info, 164, 15);
        assert!(
            second.contains(&"bark".to_string()),
            "control failed - `animal.` on the 2nd body statement should offer Dog's `bark`, got {second:?}"
        );

        // Line 158: `animal.` as the if-body's first statement.
        let first = completion_labels(session, file_symbol, file_info, 158, 15);
        assert!(
            first.contains(&"bark".to_string()),
            "`animal.` on the 1st body statement should offer Dog's `bark`, got {first:?}"
        );
    });
}

#[test]
fn break_does_not_erase_variable_after_loop() {
    with_fixture(|session, file_info, file_symbol| {
        // Line 177: `found`, after the loop whose body assigns it then breaks. If local
        // resolution breaks, lookup falls through to the fixture's module-level `found` decoy,
        // so this fails on the wrong type rather than passing by luck.
        let hover = get_hover_markdown(session, file_symbol, file_info, 177, 4);
        assert!(
            hover.as_deref().is_some_and(|h| h.contains("Animal")),
            "`found` should still resolve to Animal after the loop, got {hover:?}"
        );
    });
}

#[test]
fn and_chain_preserves_nested_or_walrus() {
    with_fixture(|session, file_info, file_symbol| {
        // Line 182: `m`, bound by the walrus inside the nested `or`.
        let hover = get_hover_markdown(session, file_symbol, file_info, 182, 8);
        assert!(
            hover.as_deref().is_some_and(|h| h.contains("Animal")),
            "walrus-bound `m` should resolve to Animal in the if body, got {hover:?}"
        );
    });
}

#[test]
fn colliding_narrowings_keep_both_types() {
    with_fixture(|session, file_info, file_symbol| {
        // Line 188: `animal` after `if not isinstance(animal, Cat): assert isinstance(animal, Dog)`
        // - Dog from the assert's branch, Cat from the negated check's fallthrough.
        assert_eq!(sorted_resolved_names(session, file_symbol, file_info, 188, 4), vec!["Cat", "Dog"]);
    });
}

#[test]
fn explicit_else_of_negated_check_narrows() {
    with_fixture(|session, file_info, file_symbol| {
        // Line 195: `animal` in the `else` of `if not isinstance(animal, Dog):`.
        assert_eq!(sorted_resolved_names(session, file_symbol, file_info, 195, 8), vec!["Dog"]);
    });
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

