//! Intergration tests of LSP features for JavaScript files and the OWL templates that back them.
//!
//! Most features require tsserver, so the suite is gated on a `TSSERVER` environment
//! variable holding the command that starts one:
//! 
//! `TSSERVER=tsserver cargo test --test test_js_owl_features`
//! 
//! Without it the test skips instead of failing, so `cargo test`
//! stays green on a machine with no TypeScript installed. `COMMUNITY_PATH` is required too.

use lsp_types::CompletionItemKind;
use odoo_ls_server::core::config::ConfigKey;
use odoo_ls_server::threads::SessionInfo;

mod setup;
mod js_owl_helpers;
use js_owl_helpers::fixture::*;
use js_owl_helpers::requests::*;
use js_owl_helpers::asserts::*;

/// The command to start tsserver with, or `None` when the suite should be skipped.
fn tsserver_command() -> Option<String> {
    match std::env::var("TSSERVER") {
        Ok(command) if !command.trim().is_empty() => Some(command),
        _ => None,
    }
}

#[test]
/// Test suite with a single server setup.
/// Requires env var `TSSERVER`, otherwise skipped.
fn test_js_owl_features() {
    let Some(tsserver_command) = tsserver_command() else {
        eprintln!("skipping test_js_owl_features: set TSSERVER to the tsserver command to run it");
        return;
    };

    let (mut odoo, mut config) = setup::setup::setup_server(true);
    config.set_str(ConfigKey::TsServerCommand, tsserver_command.clone());
    let (mut session, _tsserver_events) = setup::setup::create_init_session_with_tsserver(&mut odoo, config);

    // Check that tsserver started successfully
    assert!(
        session.sync_odoo.tsserver_bridge.is_some(),
        "tsserver did not start with TSSERVER={tsserver_command:?}"
    );
    eprintln!("COMMUNITY_PATH={:?}", std::env::var("COMMUNITY_PATH").expect("setup_server checked it already"));
    eprintln!("Odoo version={}", session.sync_odoo.version);

    let fixtures = Fixtures::init(&mut session);

    // Hover
    test_hover_in_js(&mut session, &fixtures);
    test_hover_in_template(&mut session, &fixtures);
    test_hover_in_subclass_template(&mut session, &fixtures);

    // Definition
    test_definition_in_js(&mut session, &fixtures);
    test_definition_in_template(&mut session, &fixtures);
    test_definition_in_subclass_template(&mut session, &fixtures);
    test_definition_for_template(&mut session, &fixtures);
    test_definition_for_template_jump_to_component(&mut session, &fixtures);
    test_definition_in_js_without_component(&mut session, &fixtures);

    // Completion
    test_completion_in_js(&mut session, &fixtures);
    test_completion_in_template(&mut session, &fixtures);
    test_completion_resolve(&mut session, &fixtures);
    test_completion_entries_label_details(&mut session, &fixtures);

    // References
    test_references_from_declaration(&mut session, &fixtures);
    test_references_with_unopened_files(&mut session, &fixtures);
    test_references_from_usage(&mut session, &fixtures);
    test_references_to_template(&mut session, &fixtures);
}

/// Hover in the component's own `.js`: a class field, a member assigned in `setup()` (which only
/// resolves because we hand tsserver `.js`, not `.ts`), a prop, and a helper imported from `web`.
fn test_hover_in_js(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, .. } = fixtures;
    assert_hover(session, js, "|separator = \", \"", "separator: string");
    assert_hover(session, js, "this.|punctuation.repeat", "punctuation: string");
    assert_hover(session, js, "capitalize(this.props.|name)", "name: string");
    assert_hover(session, js, "return |capitalize(", "capitalize(str: string): string");
}

/// Hover in the template, where every expression is typed through the component's virtual doc.
fn test_hover_in_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { xml, .. } = fixtures;
    assert_hover(session, xml, "t-out=\"this.props.|name\"", "name: string");
    assert_hover(session, xml, "t-out=\"this.|title\"", "title: string");
    assert_hover(session, xml, "t-out=\"this.|separator\"", "separator: string");
    assert_hover(session, xml, "t-out=\"this.|punctuation\"", "punctuation: string");
    assert_hover(session, xml, "this.|shout(", "shout(count: any): string");
    assert_hover(session, xml, "this.shout(this.props.|exclamations)", "exclamations: number");
    assert_hover(session, xml, "t-out=\"th|is.title\"", "this: Greeting");
}

/// Hover in a subclass' template, on members inherited from a base component in another file.
fn test_hover_in_subclass_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { sub_xml, .. } = fixtures;
    assert_hover(session, sub_xml, "t-out=\"this.|title\"", "title: string");
    assert_hover(session, sub_xml, "t-out=\"this.|punctuation\"", "punctuation: string");
    assert_hover(session, sub_xml, "t-out=\"this.props.|name\"", "name: string");
    assert_hover(session, sub_xml, "this.|shout(", "shout(count: any): string");
    assert_hover(session, sub_xml, "this.shout(this.|volume)", "volume: number");
    assert_hover(session, sub_xml, "t-out=\"th|is.title\"", "this: LoudGreeting");
}

/// Definition in the `.js`: imports resolved through the `@web/*` alias map
fn test_definition_in_js(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, .. } = fixtures;
    assert_definition_in(session, js, "import { |capitalize }", "web/static/src/core/utils/strings.js");
    assert_definition_in(session, js, "|clamp(count, 1, 5)", "web/static/src/core/utils/numbers.js");
}

/// Definition from a template expression back into the component `.js`.
fn test_definition_in_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml, .. } = fixtures;
    assert_definition(session, xml, "t-out=\"this.|title\"", js, "get |title()");
    assert_definition(session, xml, "t-out=\"this.|separator\"", js, "|separator = \", \"");
    assert_definition(session, xml, "t-out=\"this.|punctuation\"", js, "this.|punctuation = \"!\"");
    assert_definition(session, xml, "this.|shout(", js, "|shout(count)");
    assert_definition(session, xml, "() => this.|onClick()", js, "|onClick() {");
    // A bare `this` is asked as a *type* definition: `@this` types it but declares nothing.
    assert_definition(session, xml, "t-out=\"th|is.title\"", js, "export class |Greeting");
}

/// Definition from a subclass' template: inherited members land in the base component's file,
/// the subclass' own in its own.
fn test_definition_in_subclass_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, sub_js, sub_xml, .. } = fixtures;
    assert_definition(session, sub_xml, "t-out=\"this.|title\"", js, "get |title()");
    assert_definition(session, sub_xml, "t-out=\"this.|punctuation\"", js, "this.|punctuation = \"!\"");
    assert_definition(session, sub_xml, "this.|shout(", js, "|shout(count)");
    assert_definition(session, sub_xml, "this.shout(this.|volume)", sub_js, "|volume = 3");
    assert_definition(session, sub_xml, "t-out=\"th|is.title\"", sub_js, "export class |LoudGreeting");
}

/// `static template` → `<t t-name>` jump
/// t-call / t-inherit → `<t t-name>` jump
fn test_definition_for_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml, ext_xml, .. } = fixtures;
    let target_snippet = "|<t t-name=\"module_owl.Greeting\">";
    assert_definition(session, js, "static template = \"|module_owl.Greeting\"", xml, target_snippet);
    assert_definition(session, ext_xml, "t-call=\"|module_owl.Greeting\"", xml, target_snippet);
    assert_definition(session, ext_xml, "t-inherit=\"|module_owl.Greeting\"", xml, target_snippet);
}

/// Non-LSP behavior: asking for definition of a template at its definition site
/// leads to the matching component
fn test_definition_for_template_jump_to_component(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml , .. } = fixtures;
    assert_definition(session, xml, "<t t-name=\"|module_owl.Greeting\">", js, "export class |Greeting extends Component");
}

/// Definition should work in JS files that have no Component definitions
fn test_definition_in_js_without_component(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js_utils, report_js, .. } = fixtures;
    // definition location in same file
    assert_definition(session,
        js_utils, "if (answer < |half_answer)",
        js_utils, "const |half_answer = 21" );
    // definition location in imported file
    assert_definition(session,
        js_utils, "answer === final|_answer()",
        report_js, "export function |final_answer()" );
}

/// Completion after `this.` and `this.props.` in the component's own `.js`.
fn test_completion_in_js(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, .. } = fixtures;
    assert_completions(session, js, "this.|punctuation.repeat", &[
        ("separator", CompletionItemKind::PROPERTY),
        ("punctuation", CompletionItemKind::PROPERTY),
        ("title", CompletionItemKind::PROPERTY),
        ("shout", CompletionItemKind::METHOD),
        ("onClick", CompletionItemKind::METHOD),
    ]);
    assert_completions(session, js, "capitalize(this.props.|name)", &[
        ("name", CompletionItemKind::PROPERTY),
        ("exclamations", CompletionItemKind::PROPERTY),
    ]);
}

/// Completion inside template expressions, including the members a subclass inherits
fn test_completion_in_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { xml, sub_xml, .. } = fixtures;
    assert_completions(session, xml, "t-out=\"this.|title\"", &[
        ("separator", CompletionItemKind::PROPERTY),
        ("punctuation", CompletionItemKind::PROPERTY),
        ("title", CompletionItemKind::PROPERTY),
        ("shout", CompletionItemKind::METHOD),
    ]);
    assert_completions(session, xml, "t-out=\"this.props.|name\"", &[
        ("name", CompletionItemKind::PROPERTY),
        ("exclamations", CompletionItemKind::PROPERTY),
    ]);
    assert_completions(session, sub_xml, "t-out=\"this.|title\"", &[
        ("volume", CompletionItemKind::PROPERTY),
        ("punctuation", CompletionItemKind::PROPERTY),
        ("title", CompletionItemKind::PROPERTY),
        ("shout", CompletionItemKind::METHOD),
    ]);
}

/// Assert the signature and docs of a completion, which reach the client only through a
/// `completionItem/resolve` round trip
fn test_completion_resolve(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml, .. } = fixtures;
    let shout = resolved_completion(session, js, "this.|punctuation.repeat", "shout");
    assert_detail(&shout, "shout(count: any): string");
    assert!(shout.documentation.is_some(), "resolve should carry shout's JSDoc, got none");

    let shout = resolved_completion(session, xml, "t-out=\"this.|title\"", "shout");
    assert_detail(&shout, "shout(count: any): string");
    assert!(shout.documentation.is_some(), "resolve should carry shout's JSDoc, got none");
    // Its edits would be in virtual-doc coordinates, which would corrupt the XML if applied.
    assert!(shout.additional_text_edits.is_none(), "a template completion must carry no edits");
}

fn test_completion_entries_label_details(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js_utils, .. } = fixtures;
    assert_completions_label_details(session, js_utils, "const service = useS|", &[
        ("useService", "@web/core/utils/hooks"),
        ("useSpellCheck", "@web/core/utils/hooks"),
    ]);
}

/// Find-references from a member's declaration
fn test_references_from_declaration(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml, sub_xml, .. } = fixtures;
    assert_references(session, js, "this.|punctuation = \"!\"", &[
        (js, "this.|punctuation = \"!\""), // declaration site is a reference too
        (js, "this.|punctuation.repeat"),
        (js, "this.|punctuation = \"?\""),
        (xml, "t-out=\"this.|punctuation\""),
        (sub_xml, "t-out=\"this.|punctuation\""),
    ]);
}

/// Find-references from a member's declarations.
/// Some of the references are in files that the test never opens (nor are
/// imported by the opened files), so they are not part of tsserver's project
/// until the import graph stages them.
fn test_references_with_unopened_files(session: &mut SessionInfo, fixtures:
&Fixtures) {
    let Fixtures { js, xml, sub_xml, quiet_js, report_js, .. } = fixtures;
    assert_references(session, js, "|shout(count) {", &[
        (js, "|shout(count) {"), // declaration site is a reference too
        (xml, "this.|shout("),
        (sub_xml, "this.|shout("),
        // unopened files
        (quiet_js, "this.|shout(1)"),
        (report_js, "greeting.|shout(2)"),
    ]);
}

/// Find-references requested at usage sites (not at declaration)
fn test_references_from_usage(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml, sub_js, sub_xml, quiet_js, report_js, .. } = fixtures;
    assert_references(session, js, "this.|punctuation.repeat", &[
        (js, "this.|punctuation = \"!\""), // declaration site
        (js, "this.|punctuation.repeat"),
        (js, "this.|punctuation = \"?\""),
        (xml, "t-out=\"this.|punctuation\""),
        (sub_xml, "t-out=\"this.|punctuation\""),
    ]);
    assert_references(session, xml, "t-out=\"this.|separator\"", &[
        (js, "|separator = \", \""), // declaration site
        (xml, "t-out=\"this.|separator\""),
        (sub_js, "this.|separator.repeat"),
        (report_js, "greeting.|separator"),
    ]);
    assert_references(session, sub_js, "this.|separator.repeat", &[
        (js, "|separator = \", \""),
        (xml, "t-out=\"this.|separator\""),
        (sub_js, "this.|separator.repeat"),
        (report_js, "greeting.|separator"),
    ]);
    assert_references(session, sub_xml, "this.|shout(", &[
        (js, "|shout(count)"), // declaration site
        (xml, "this.|shout("),
        (sub_xml, "this.|shout("),
        (quiet_js, "this.|shout(1)"),
        (report_js, "greeting.|shout(2)"),
    ]);
}

/// References to a template name.
/// The file carrying the two latter sites is never opened.
fn test_references_to_template(session: &mut SessionInfo, fixtures: &Fixtures) {
    let Fixtures { js, xml, ext_xml, .. } = fixtures;
    let sites: &[(&FixtureFile, &str)] = &[
        (xml, "t-name=\"|module_owl.Greeting\""), // definition site
        (js, "static template = \"|module_owl.Greeting\""),
        (ext_xml, "t-call=\"|module_owl.Greeting\""),
        (ext_xml, "t-inherit=\"|module_owl.Greeting\""),
    ];
    assert_references(session, js, "static template = \"|module_owl.Greeting\"", sites);
    assert_references(session, xml, "t-name=\"|module_owl.Greeting\"", sites);
}
