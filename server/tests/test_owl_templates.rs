use std::path::PathBuf;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::features::js_completion::owl_completion;
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
    let mut session = setup::setup::create_init_session(&mut odoo, config);

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
        locs[0].target_uri.to_string()
    );
}

/// Goto-definition from an OWL directive attribute value in XML (`t-on-click`, `t-esc`)
/// should jump to the matching JS component member (method, getter, or reactive state field).
#[test]
fn test_goto_definition_from_xml_attrs_to_js_members() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let xml_path = module_owl_path(&["static", "src", "counter", "counter.xml"]);
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(&mut session, &PathBuf::from(&xml_path))
        .expect("XML asset file should have a symbol after init");
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&xml_path).unwrap();

    // line 3: `<button t-on-click="increment">` — cursor inside "increment".
    let locs = test_utils::get_definition_locs(&mut session, file_symbol, &file_info, 3, 35);
    assert_eq!(locs.len(), 1, "expected one definition for t-on-click=\"increment\", got {:?}", locs);
    assert!(locs[0].target_uri.to_string().ends_with("counter.js"));
    assert_eq!(locs[0].target_range.start.line, 11);
    assert_eq!(locs[0].target_range.start.character, 4);
    assert_eq!(locs[0].target_range.end.character, 13);

    // line 4: `<span t-esc="state.value"/>` — cursor inside "state".
    let locs = test_utils::get_definition_locs(&mut session, file_symbol, &file_info, 4, 27);
    assert_eq!(locs.len(), 1, "expected one definition for t-esc=\"state.value\", got {:?}", locs);
    assert!(locs[0].target_uri.to_string().ends_with("counter.js"));
    assert_eq!(locs[0].target_range.start.line, 8);
    assert_eq!(locs[0].target_range.start.character, 13);
    assert_eq!(locs[0].target_range.end.character, 18);

    // line 5: `<span t-esc="doubled"/>` — cursor inside "doubled".
    let locs = test_utils::get_definition_locs(&mut session, file_symbol, &file_info, 5, 28);
    assert_eq!(locs.len(), 1, "expected one definition for t-esc=\"doubled\", got {:?}", locs);
    assert!(locs[0].target_uri.to_string().ends_with("counter.js"));
    assert_eq!(locs[0].target_range.start.line, 15);
    assert_eq!(locs[0].target_range.start.character, 8);
    assert_eq!(locs[0].target_range.end.character, 15);
}

/// Completion inside an OWL directive attribute (`t-esc="this.state.<cursor>"`) should offer
/// the members of the nested reactive state object built from `useState({ value: 0 })`.
#[test]
fn test_owl_completion_nested_state_member() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let xml_path = module_owl_path(&["static", "src", "counter", "counter.xml"]);
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&xml_path).unwrap();

    // line 6: `<span t-esc="this.state.value"/>` — cursor right after "this.state.".
    let items = owl_completion(&mut session, &file_info, 6, 36)
        .expect("expected completion items for this.state.<cursor>");
    let labels: Vec<_> = items.iter().map(|i| i.label.clone()).collect();
    assert!(labels.contains(&"value".to_string()), "expected 'value' in completions, got {:?}", labels);
}
