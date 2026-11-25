#[path = "../common/mod.rs"]
mod common;

mod self_propagation_tests;
mod test_arch_eval_hooks;
mod test_completion;
mod test_controller;
mod test_env_attributes;
mod test_env_subscript;
mod test_get_symbol;
mod test_hooks;
mod test_language;
mod test_ls;
mod test_module_lifecycle;
mod test_references;
mod test_semantic_tokens;
mod test_start_server;
mod test_xml_fields;

use crate::{
    common::{build_once_run_isolated, setup_server},
    self_propagation_tests::test_self_propagation,
    test_arch_eval_hooks::{
        test_base_model_env_hook, test_ids_and_id_field_hooks, test_ir_rule_global_field_hook,
        test_odoo_init_symbol_hooks, test_registry_hooks, test_relational_field_get_hooks,
        test_relational_field_hover_shows_comodel, test_scalar_field_get_hooks, test_self_returning_chain_hooks,
    },
    test_controller::{test_request_env_definition, test_request_env_subscript, test_request_type_hover},
    test_env_attributes::{test_env_attributes_resolve, test_env_user_through_variable_resolves},
    test_env_subscript::test_env_subscript_after_getitem_invalidation,
    test_get_symbol::{
        test_csv_field_ranges, test_csv_quoted_commas, test_definition, test_definition_csv,
        test_hover_inverse_name_o2m, test_hover_on_model_field_and_method, test_hover_on_namespace_and_module,
        test_lambda_parameter_scoping, test_model_subscription, test_xml_definition, test_xml_hover,
    },
    test_hooks::test_search_eval_hook,
    test_ls::test_structure,
};

#[test]
fn test_arch_eval_hooks() {
    let (odoo, config) = setup_server(true);
    build_once_run_isolated(odoo, config, &[
        &test_self_returning_chain_hooks,
        &test_registry_hooks,
        &test_ids_and_id_field_hooks,
        &test_odoo_init_symbol_hooks,
        &test_ir_rule_global_field_hook,
        &test_base_model_env_hook,
        &test_scalar_field_get_hooks,
        &test_relational_field_get_hooks,
        &test_relational_field_hover_shows_comodel,
        &test_search_eval_hook,
        &test_env_attributes_resolve,
        &test_env_user_through_variable_resolves,
        &|mut s| test_request_type_hover(&mut s),
        &|mut s| test_request_env_definition(&mut s),
        &|mut s| test_request_env_subscript(&mut s),
        &test_env_subscript_after_getitem_invalidation,
    ]);
}

#[test]
fn test_get_symbol() {
    let (odoo, config) = setup_server(true);
    build_once_run_isolated(odoo, config, &[
        &test_structure,
        &test_hover_on_model_field_and_method,
        &test_hover_inverse_name_o2m,
        &test_hover_on_namespace_and_module,
        &test_definition_csv,
        &test_model_subscription,
        &test_self_propagation,
        &test_lambda_parameter_scoping,
        &test_csv_quoted_commas,
        &test_csv_field_ranges,
        &test_xml_hover,
        &test_xml_definition,
        &test_definition,
    ]);
}
