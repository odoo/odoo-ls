//! Tests that fit no other test crate: regressions.

#[path = "../common/mod.rs"]
mod common;

mod test_addon_entry_lost_symbol;
mod test_build_cycle;
mod test_did_close_cleanup;
mod test_entry_point_teardown;
mod test_file_delete;
mod test_nested_file_pending_delete_gap;
mod test_queued_dependency_missing_file_info;
mod test_stale_node_index;
mod test_untitled_pending_delete_unaffected;
mod test_workspace_symbol_duplicates;
