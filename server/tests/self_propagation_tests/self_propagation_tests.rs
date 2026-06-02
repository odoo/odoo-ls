use std::path::PathBuf;

use odoo_ls_server::{core::{odoo::SyncOdoo, symbols::{symbol_keys::SymbolKey, storage::SymbolTable}}, utils::PathSanitizer};

use crate::{setup::setup, test_utils};

#[test]
fn test_model_subscription() {
    // Setup: Get the symbol for BaseTestModel and verify its existence
    let (mut odoo, config) = setup::setup_server(true);
    let mut session = setup::create_init_session(&mut odoo, config);
    let test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("self_propagation_module").join("models").join("all_models.py").sanitize();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(
        &mut session,
        &PathBuf::from(&test_file)
    ) else {
        panic!("Failed to get file symbol");
    };
    let follow_evaluation_refs = |syms: Vec<SymbolKey>, session: &mut odoo_ls_server::threads::SessionInfo<'_>| syms.iter().flat_map(|&sym| {
        match session.st().evaluations(sym).cloned() {
            Some(evals) => evals.into_iter().flat_map(|eval|
                SymbolTable::follow_ref(
                    &eval.symbol.get_symbol(session, None, &mut vec![], None),
                    session,
                    None,
                    false,
                    false,
                    None,
                    None
                )
            )
            .collect::<Vec<_>>()
            .iter().flat_map(|sym_ptr|
                sym_ptr.upgrade_weak(session.st())
            )
            .collect::<Vec<_>>(),
            None => vec![]
        }
    }).collect::<Vec<_>>();

    let diagnostics = setup::get_diagnostics_for_path(&mut session, &test_file);
    // Verify that there are no OLS03011 on line 18 due to wrong self propagation due to with_user in search_in_a method of ModelA
    let line_diagnostics = test_utils::diag_on_line(&diagnostics, 17);
    assert_eq!(
        line_diagnostics.len(),
        0,
        "Expected no diagnostics on line 18, but found some: {:?}",
        line_diagnostics
    );


    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let model_a_sym = session.st().get_symbol(file_symbol.into(),(&[], &["ModelA"]), u32::MAX);
    assert_eq!(model_a_sym.len(), 1, "Expected 1 symbol for ModelA");
    let model_b_sym = session.st().get_symbol(file_symbol.into(),(&[], &["ModelB"]), u32::MAX);
    assert_eq!(model_b_sym.len(), 1, "Expected 1 symbol for ModelB");

    let create_new_in_model_a = follow_evaluation_refs(
        test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 8, 14),
        &mut session,
    );
    assert!(
        create_new_in_model_a.iter().any(|sym| sym == &model_a_sym[0]),
        "Expected to find ModelA symbol"
    );

    let create_from_self_env_in_model_a = follow_evaluation_refs(
        test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 11, 14),
        &mut session,
    );
    assert!(
        create_from_self_env_in_model_a.iter().any(|sym| sym == &model_a_sym[0]),
        "Expected to find ModelA symbol"
    );

    let search_in_a_in_model_a = follow_evaluation_refs(
        test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 14, 14),
        &mut session,
    );
    assert!(
        search_in_a_in_model_a.iter().any(|sym| sym == &model_a_sym[0]),
        "Expected to find ModelA symbol"
    );

    let create_new_in_model_b = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 27, 24);
    assert!(
        create_new_in_model_b.iter().any(|sym| sym == &model_b_sym[0]),
        "Expected to find ModelB symbol"
    );

    let create_from_self_env_in_model_b = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 28, 34);
    assert!(
        create_from_self_env_in_model_b.iter().any(|sym| sym == &model_a_sym[0]),
        "Expected to find ModelA symbol"
    );

    // Test on normal classes

    let a_sym = session.st().get_symbol(file_symbol.into(), (&[], &["A"]), u32::MAX);
    assert_eq!(a_sym.len(), 1, "Expected 1 symbol for A");
    let b_sym = session.st().get_symbol(file_symbol.into(), (&[], &["B"]), u32::MAX);
    assert_eq!(b_sym.len(), 1, "Expected 1 symbol for B");

    let method_self_in_a = follow_evaluation_refs(
        test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 31, 14),
        &mut session,
    );
    assert!(
        method_self_in_a.iter().any(|sym| sym == &a_sym[0]),
        "Expected to find A symbol"
    );

    let method_hard_a_in_a = follow_evaluation_refs(
        test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 33, 14),
        &mut session,
    );
    assert!(
        method_hard_a_in_a.iter().any(|sym| sym == &a_sym[0]),
        "Expected to find A symbol"
    );

    let method_b_self_in_b = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 38, 32);
    assert!(
        method_b_self_in_b.iter().any(|sym| sym == &b_sym[0]),
        "Expected to find B symbol"
    );

    let method_b_hard_a_in_b = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 41, 34);
    assert!(
        method_b_hard_a_in_b.iter().any(|sym| sym == &a_sym[0]),
        "Expected to find A symbol"
    );
}
