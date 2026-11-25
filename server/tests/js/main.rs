#[path = "../common/mod.rs"]
mod common;
mod js_owl_helpers;

mod test_js_arch_builder;
mod test_js_lifecycle;
mod test_js_module_scope;
mod test_js_owl_features;
mod test_owl_templates;
mod test_tsserver_paths;

use crate::{
    common::{build_once_run_isolated, setup_server},
    test_js_module_scope::{
        test_importable_files_are_scoped_to_manifest_depends, test_importable_prefixes_cover_the_same_closure,
        test_only_importable_files_are_roots, test_out_of_scope_import_is_flagged,
        test_type_files_are_scoped_to_manifest_depends,
    },
    test_owl_templates::{
        test_goto_definition_from_js_template_to_xml, test_super_class_resolves_through_its_import,
        test_template_and_component_are_linked,
    },
};

#[test]
fn test_js_without_tsserver() {
    let (odoo, config) = setup_server(true);
    build_once_run_isolated(odoo, config, &[
        &|s| test_template_and_component_are_linked(&s),
        &|s| test_super_class_resolves_through_its_import(&s),
        &|mut s| test_goto_definition_from_js_template_to_xml(&mut s),
        &|s| test_type_files_are_scoped_to_manifest_depends(&s),
        &|s| test_importable_files_are_scoped_to_manifest_depends(&s),
        &|s| test_only_importable_files_are_roots(&s),
        &|s| test_importable_prefixes_cover_the_same_closure(&s),
        &|mut s| test_out_of_scope_import_is_flagged(&mut s),
    ]);
}
