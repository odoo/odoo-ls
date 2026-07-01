use std::path::PathBuf;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;
mod test_utils;

fn module_owl_path(rel: &[&str]) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_owl");
    for part in rel {
        path = path.join(part);
    }
    path.sanitize()
}

/// After a full build, the JS `static template = "module_owl.Counter"` and the XML
/// `<t t-name="module_owl.Counter">` should be linked in both directions: the JS side
/// records which class renders the template, and the XML side registers the template
/// symbol so JS→XML lookups resolve.
#[test]
fn test_template_and_component_are_linked() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let session = setup::setup::create_init_session(&mut odoo, config);

    let class_name = session
        .sync_odoo
        .js_component_by_template
        .get("module_owl.Counter")
        .cloned()
        .expect("JS static template should register module_owl.Counter -> Counter");
    assert_eq!(class_name, "Counter");

    let descriptor = session
        .sync_odoo
        .component_descriptors
        .get(&class_name)
        .expect("Counter component descriptor should exist");
    assert!(descriptor.find_member("increment").is_some());
    assert!(descriptor.find_member("doubled").is_some());
    assert!(descriptor.find_member("state").is_some());

    let templates = session
        .sync_odoo
        .js_templates
        .get("module_owl.Counter")
        .expect("XML t-name=module_owl.Counter should be registered");
    assert!(
        !templates.is_empty(&session.sync_odoo.symbol_table),
        "module_owl.Counter should resolve to at least one XML template symbol"
    );
}

/// Goto-definition from the `static template = "module_owl.Counter"` string in the JS
/// component should jump to the `<t t-name="module_owl.Counter">` node in the XML file.
#[test]
fn test_goto_definition_from_js_template_to_xml() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let js_path = module_owl_path(&["static", "src", "counter", "counter.js"]);
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, &PathBuf::from(&js_path))
        .expect("JS asset file should have a symbol after init");
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&js_path).unwrap();

    // line 4: `    static template = "module_owl.Counter";` — cursor inside the string content.
    let locs = test_utils::get_definition_locs(&mut session, file_symbol, &file_info, 4, 25);
    assert!(!locs.is_empty(), "expected a definition for the OWL template string");
    assert!(
        locs[0].target_uri.to_string().ends_with("counter.xml"),
        "expected definition to land in counter.xml, got {}",
        *locs[0].target_uri
    );
}
