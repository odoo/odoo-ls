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
    test_cached_property::test_cached_and_lazy_property_are_properties,
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
