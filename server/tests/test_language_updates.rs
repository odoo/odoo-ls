use std::path::PathBuf;

use lsp_types::{TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;
use setup::setup::{create_init_session, get_diagnostics_for_path, setup_server};

/// Helper to check if diagnostics contain a specific code
fn has_diagnostic_code(diagnostics: &[lsp_types::Diagnostic], code: &str) -> bool {
    diagnostics.iter().any(|d| match &d.code {
        Some(lsp_types::NumberOrString::String(s)) => s == code,
        _ => false,
    })
}

/// Helper to assert a language is registered (or not)
fn assert_language_registered(session: &mut SessionInfo<'_>, lang: &str, should_exist: bool) {
    let languages = session.sync_odoo._get_languages();
    if should_exist {
        assert!(
            languages.contains(lang),
            "Language {} should be registered. Available: {:?}",
            lang,
            languages
        );
    } else {
        assert!(
            !languages.contains(lang),
            "Language {} should NOT be registered. Available: {:?}",
            lang,
            languages
        );
    }
}

/// Helper to simulate a file change via LSP didChange
fn simulate_file_change(
    session: &mut SessionInfo<'_>,
    path: &PathBuf,
    content: &str,
    version: i32,
) {
    let uri = FileMgr::pathname2uri(&path.sanitize());
    let params = lsp_types::DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier { uri, version },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: content.to_string(),
        }],
    };
    Odoo::handle_did_change(session, params);
}

/// This test verifies:
/// 1. Language registry updates when res_lang.xml changes
/// 2. XML diagnostics refresh when languages are added/removed
/// 3. New files can introduce languages and trigger diagnostic refresh
#[test]
fn test_language_validation() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let lang_xml_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_lang_test/data/res_lang.xml");
    let items_xml_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_lang_test/data/lang_test_items.xml");
    let extra_lang_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_lang_test/data/extra_languages.xml");

    // ========================================
    // PART 1: Registry updates on file change
    // ========================================

    // Verify initial languages are registered
    assert_language_registered(&mut session, "fr_TEST", true);
    assert_language_registered(&mut session, "de_TEST", true);
    assert_language_registered(&mut session, "es_TEST", false);

    // Modify the file: remove de_TEST, add es_TEST
    let new_content = r#"<?xml version="1.0" encoding="utf-8"?>
<odoo>
    <record id="lang_test_fr" model="res.lang">
        <field name="name">French (Test)</field>
        <field name="code">fr_TEST</field>
        <field name="iso_code">fr</field>
    </record>
    <record id="lang_test_es" model="res.lang">
        <field name="name">Spanish (Test)</field>
        <field name="code">es_TEST</field>
        <field name="iso_code">es</field>
    </record>
</odoo>
"#;

    simulate_file_change(&mut session, &lang_xml_path, new_content, 2);

    // Verify languages are updated correctly
    assert_language_registered(&mut session, "fr_TEST", true); // unchanged
    assert_language_registered(&mut session, "es_TEST", true); // newly added
    assert_language_registered(&mut session, "de_TEST", false); // removed

    // ========================================
    // PART 2: Diagnostic refresh on language change
    // ========================================

    // Restore original content first (fr_TEST, de_TEST)
    let original_content = r#"<?xml version="1.0" encoding="utf-8"?>
<odoo>
    <record id="lang_test_fr" model="res.lang">
        <field name="name">French (Test)</field>
        <field name="code">fr_TEST</field>
        <field name="iso_code">fr</field>
    </record>
    <record id="lang_test_de" model="res.lang">
        <field name="name">German (Test)</field>
        <field name="code">de_TEST</field>
        <field name="iso_code">de</field>
    </record>
</odoo>
"#;
    simulate_file_change(&mut session, &lang_xml_path, original_content, 3);

    // Verify initial state - nl_WV doesn't exist, so OLS05058 should be present
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        has_diagnostic_code(&diagnostics, "OLS05058"),
        "Initial state: OLS05058 should be present for nl_WV language. Diagnostics: {:?}",
        diagnostics
    );

    // Add nl_WV language by updating res_lang.xml
    let with_nl_wv_content = r#"<?xml version="1.0" encoding="utf-8"?>
<odoo>
    <record id="lang_test_fr" model="res.lang">
        <field name="name">French (Test)</field>
        <field name="code">fr_TEST</field>
        <field name="iso_code">fr</field>
    </record>
    <record id="lang_test_de" model="res.lang">
        <field name="name">German (Test)</field>
        <field name="code">de_TEST</field>
        <field name="iso_code">de</field>
    </record>
    <record id="lang_test_nl_wv" model="res.lang">
        <field name="name">West Flemish</field>
        <field name="code">nl_WV</field>
        <field name="iso_code">nl</field>
    </record>
</odoo>
"#;

    simulate_file_change(&mut session, &lang_xml_path, with_nl_wv_content, 4);

    // Verify nl_WV is now registered
    assert_language_registered(&mut session, "nl_WV", true);

    // Check that OLS05058 diagnostic is cleared
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        !has_diagnostic_code(&diagnostics, "OLS05058"),
        "After adding nl_WV: OLS05058 should be CLEARED. Diagnostics: {:?}",
        diagnostics
    );

    // Remove nl_WV language
    simulate_file_change(&mut session, &lang_xml_path, original_content, 5);

    // Verify nl_WV is removed and diagnostic reappears
    assert_language_registered(&mut session, "nl_WV", false);

    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        has_diagnostic_code(&diagnostics, "OLS05058"),
        "After removing nl_WV: OLS05058 should REAPPEAR. Diagnostics: {:?}",
        diagnostics
    );

    // ========================================
    // PART 3: Different file as language source
    // ========================================

    // Now we'll add nl_WV via a separate file to test that language updates
    // work when a new source of res.lang records is introduced.

    // Verify nl_WV is still not registered
    assert_language_registered(&mut session, "nl_WV", false);

    // Add nl_WV via extra_languages.xml (simulating user editing the file)
    let new_file_content = r#"<?xml version="1.0" encoding="utf-8"?>
<odoo>
    <record id="lang_extra_nl_wv" model="res.lang">
        <field name="name">West Flemish</field>
        <field name="code">nl_WV</field>
        <field name="iso_code">nl</field>
    </record>
</odoo>
"#;

    simulate_file_change(&mut session, &extra_lang_path, new_file_content, 2);

    // Verify nl_WV is now registered from the additional source
    assert_language_registered(&mut session, "nl_WV", true);

    // Verify OLS05058 is cleared
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        !has_diagnostic_code(&diagnostics, "OLS05058"),
        "After adding nl_WV via extra_languages.xml: OLS05058 should be CLEARED. Diagnostics: {:?}",
        diagnostics
    );
}
