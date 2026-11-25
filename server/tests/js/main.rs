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
    test_js_module_scope::test_js_module_scope,
    test_owl_templates::test_owl_templates,
};

#[test]
fn test_js_without_tsserver() {
    let (odoo, config) = setup_server(true);
    build_once_run_isolated(odoo, config, &[
        &test_owl_templates,
        &test_js_module_scope,
    ]);
}
