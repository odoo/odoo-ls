#[path = "../common/mod.rs"]
mod common;

mod test_basics;
mod test_buildsteps;
mod test_cached_property;
mod test_completion_trigger_kind;
mod test_delete_dir_while_open;
mod test_follow_ref;
mod test_standalone_file_lifecycle;
mod test_type_hints;
mod test_unicode;
mod test_unpack;
mod test_untitled;

use crate::{
    common::{build_once_run_isolated, setup_server},
    test_basics::{
        test_ann_assign_invalid_target, test_assigns, test_custom_entry_point, test_no_main_entry,
        test_sections, test_star_import_on_disk_dir_does_not_panic,
    },
    test_buildsteps::{
        test_build_now_on_previous_step_is_noop, test_buildsteps_steps,
        test_queue_is_noop_once_step_is_done, test_set_build_status_on_previous_step_to_done_is_noop,
    },
    test_cached_property::test_cached_and_lazy_property_are_properties,
    test_completion_trigger_kind::{
        test_comma_position_invoked_still_completes, test_comma_trigger_char_returns_nothing,
        test_dot_trigger_char_still_completes, test_missing_context_behaves_like_invoked,
    },
    test_delete_dir_while_open::{
        test_delete_open_file_then_close_it_clears_everything, test_delete_open_file_then_edit_it,
        test_delete_open_file_then_recreate_it_keeps_it,
        test_directory_delete_with_nested_open_file_is_not_deferred,
        test_feature_request_on_deleted_open_file_does_not_resurrect_a_phantom_entry,
    },
    test_follow_ref::test_follow_ref,
    test_standalone_file_lifecycle::{
        test_did_delete_on_never_opened_standalone_file_is_a_noop,
        test_hover_on_unopened_standalone_file_builds_lazily,
        test_standalone_python_create_open_close_lifecycle,
        test_standalone_python_rename_defers_entry_to_reopen,
    },
    test_type_hints::{test_function_return_type_hint, test_with_statement_type_from_enter},
    test_unicode::test_unicode_file_lifecycle,
    test_unpack::test_unpack,
    test_untitled::test_untitled_file_lifecycle,
};

#[test]
fn test_expressions() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_cached_and_lazy_property_are_properties,
        &test_follow_ref,
        &test_function_return_type_hint,
        &test_with_statement_type_from_enter,
        &test_unpack,
    ]);
}

#[test]
fn test_file_lifecycle() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_unicode_file_lifecycle,
        &test_untitled_file_lifecycle,
        &test_standalone_python_create_open_close_lifecycle,
        &test_standalone_python_rename_defers_entry_to_reopen,
        &test_hover_on_unopened_standalone_file_builds_lazily,
        &test_did_delete_on_never_opened_standalone_file_is_a_noop,
    ]);
}

#[test]
fn test_basics() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_no_main_entry,
        &test_custom_entry_point,
        &test_assigns,
        &test_ann_assign_invalid_target,
        &test_sections,
        &test_star_import_on_disk_dir_does_not_panic,
    ]);
}

#[test]
fn test_buildsteps() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_build_now_on_previous_step_is_noop,
        &test_set_build_status_on_previous_step_to_done_is_noop,
        &test_queue_is_noop_once_step_is_done,
        &test_buildsteps_steps,
    ]);
}

#[test]
fn test_completion_trigger_kind() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_comma_trigger_char_returns_nothing,
        &test_comma_position_invoked_still_completes,
        &test_missing_context_behaves_like_invoked,
        &test_dot_trigger_char_still_completes,
    ]);
}

#[test]
fn test_delete_dir_while_open() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_delete_open_file_then_edit_it,
        &test_delete_open_file_then_close_it_clears_everything,
        &test_feature_request_on_deleted_open_file_does_not_resurrect_a_phantom_entry,
        &test_delete_open_file_then_recreate_it_keeps_it,
        &test_directory_delete_with_nested_open_file_is_not_deferred,
    ]);
}
