use std::path::{Path, PathBuf};

use odoo_ls_server::core::js_arch_builder::ComponentDescriptor;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::threads::SessionInfo;
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

// shared setup
#[test]
fn test_owl_templates() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    test_template_and_component_are_linked(&session);
    test_super_class_resolves_through_its_import(&session);
    test_goto_definition_from_js_template_to_xml(&mut session);
}

/// After a full build, the JS `static template = "module_owl.Counter"` and the XML
/// `<t t-name="module_owl.Counter">` should be linked in both directions: the JS side
/// records which class renders the template, and the XML side registers the template
/// symbol so JS→XML lookups resolve.
fn test_template_and_component_are_linked(session: &SessionInfo) {
    let component = session.sync_odoo.component_mgr.component_for_template(session, "module_owl.Counter")
        .expect("JS static template should register module_owl.Counter -> Counter");
    assert_eq!(component.class_name, "Counter");
    assert!(
        component.file_path.ends_with("counter/counter.js"),
        "expected the descriptor declared in counter.js, got {}",
        component.file_path,
    );

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

/// A super class is resolved through its import specifier, so both spellings must land on the
/// file that declares the class. Only the `@{module}/…` one goes through the module lookup —
/// the form real Odoo code uses to inherit across modules.
fn test_super_class_resolves_through_its_import(session: &SessionInfo) {
    for (file, class) in [("loud_greeting.js", "LoudGreeting"), ("shouty_greeting.js", "ShoutyGreeting")] {
        let base = super_class_of(session, file, class);
        assert_eq!(base.class_name, "Greeting", "{class}");
        assert!(base.file_path.ends_with("greeting/greeting.js"), "{class} extends {}", base.file_path);
    }
}

fn super_class_of<'a>(session: &'a SessionInfo, file: &str, class: &str) -> &'a ComponentDescriptor {
    let component_mgr = &session.sync_odoo.component_mgr;
    let path = module_owl_path(&["static", "src", "greeting", file]);
    let component = component_mgr
        .get_component(&path, class)
        .unwrap_or_else(|| panic!("{class} should be indexed in {file}"));
    component_mgr
        .get_super(session, component)
        .unwrap_or_else(|| panic!("the super class of {class} should resolve"))
}

/// Goto-definition from the `static template = "module_owl.Counter"` string in the JS
/// component should jump to the `<t t-name="module_owl.Counter">` node in the XML file.
fn test_goto_definition_from_js_template_to_xml(session: &mut SessionInfo) {
    let js_path = module_owl_path(&["static", "src", "counter", "counter.js"]);
    let file_symbol = SyncOdoo::get_symbol_of_opened_file(session, Path::new(&js_path))
        .expect("JS asset file should have a symbol after init");
    let file_info = session.file_mgr().get_file_info(&js_path).unwrap();

    // line 4: `    static template = "module_owl.Counter";` — cursor inside the string content.
    let locs = test_utils::get_definition_locs(session, file_symbol, file_info, 4, 25);
    assert!(!locs.is_empty(), "expected a definition for the OWL template string");
    assert!(
        locs[0].target_uri.to_string().ends_with("counter.xml"),
        "expected definition to land in counter.xml, got {}",
        *locs[0].target_uri
    );
}
