//! Tests that fit no other test crate: regressions, tests of the test helpers.

#[path = "../common/mod.rs"]
mod common;

mod regression_tests {
    pub mod test_addon_entry_lost_symbol;
    pub mod test_build_cycle;
    pub mod test_did_close_cleanup;
    pub mod test_entry_point_teardown;
    pub mod test_file_delete;
    pub mod test_nested_file_pending_delete_gap;
    pub mod test_queued_dependency_missing_file_info;
    pub mod test_stale_node_index;
    pub mod test_untitled_pending_delete_unaffected;
    pub mod test_workspace_symbol_duplicates;
}
mod test_runner;

use crate::common::{build_once_run_isolated, setup_server};
use crate::regression_tests::{
    test_addon_entry_lost_symbol::test_addon_entry_losing_its_symbol_does_not_panic,
    test_did_close_cleanup::{
        test_closing_a_standalone_file_still_releases_its_own_entry, test_closing_an_external_file_drops_its_cache_but_keeps_its_symbols,
        test_closing_builtins_pyi_keeps_builtins, test_is_external_path,
    },
    test_entry_point_teardown::test_custom_entry_point_teardown_frees_entry_and_root,
    test_file_delete::test_delete_file_path_does_not_panic,
    test_nested_file_pending_delete_gap::test_nested_bare_dir_file_is_kept_on_close_even_though_not_flagged_in_workspace,
    test_queued_dependency_missing_file_info::test_dependency_queued_alongside_its_external_duplicate_does_not_panic,
    test_stale_node_index::test_stale_node_index_on_method_does_not_panic,
    test_untitled_pending_delete_unaffected::test_untitled_buffer_close_does_not_crash,
};

#[test]
fn test_regressions() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_addon_entry_losing_its_symbol_does_not_panic,
        &test_closing_builtins_pyi_keeps_builtins,
        &test_closing_a_standalone_file_still_releases_its_own_entry,
        &test_is_external_path,
        &test_closing_an_external_file_drops_its_cache_but_keeps_its_symbols,
        &test_custom_entry_point_teardown_frees_entry_and_root,
        &test_delete_file_path_does_not_panic,
        &test_nested_bare_dir_file_is_kept_on_close_even_though_not_flagged_in_workspace,
        &test_dependency_queued_alongside_its_external_duplicate_does_not_panic,
        &test_stale_node_index_on_method_does_not_panic,
        &test_untitled_buffer_close_does_not_crash,
    ]);
}
