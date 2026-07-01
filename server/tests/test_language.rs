use odoo_ls_server::utils::HashMap;
use std::path::PathBuf;

use lsp_types::{NumberOrString, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier};
use odoo_ls_server::core::config::{ConfigView, ConfigKey};
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
    path: &Path,
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

    // Verify initial state - nl_WV doesn't exist, so OLS05068 should be present
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        has_diagnostic_code(&diagnostics, "OLS05068"),
        "Initial state: OLS05068 should be present for nl_WV language. Diagnostics: {:?}",
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

    // Check that OLS05068 diagnostic is cleared
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        !has_diagnostic_code(&diagnostics, "OLS05068"),
        "After adding nl_WV: OLS05068 should be CLEARED. Diagnostics: {:?}",
        diagnostics
    );

    // Remove nl_WV language
    simulate_file_change(&mut session, &lang_xml_path, original_content, 5);

    // Verify nl_WV is removed and diagnostic reappears
    assert_language_registered(&mut session, "nl_WV", false);

    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        has_diagnostic_code(&diagnostics, "OLS05068"),
        "After removing nl_WV: OLS05068 should REAPPEAR. Diagnostics: {:?}",
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

    // Verify OLS05068 is cleared
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path.sanitize());
    assert!(
        !has_diagnostic_code(&diagnostics, "OLS05068"),
        "After adding nl_WV via extra_languages.xml: OLS05068 should be CLEARED. Diagnostics: {:?}",
        diagnostics
    );
}

/// Test that additional_languages config option makes languages recognized
/// without needing res.lang records in data files.
#[test]
fn test_additional_languages_config() {
    let (mut odoo, mut config) = setup_server(false);
    config.set_string_list(ConfigKey::AdditionalLanguages, ["zz".to_string()]);
    let session = create_init_session(&mut odoo, config);

    let languages = session.sync_odoo._get_languages();
    assert!(
        languages.contains("zz"),
        "additional_languages should include zz. Available: {:?}",
        languages
    );
}

/// Test that all language codes from res.lang.csv are recognized as valid.
/// This ensures no false positives due to error in parsing the main source of language codes.
#[test]
fn test_no_false_positives_from_res_lang_csv() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_lang_test/data/all_langs_test.xml")
        .sanitize();

    let diagnostics = get_diagnostics_for_path(&mut session, &path);

    let ols05068: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            matches!(&d.code, Some(NumberOrString::String(s)) if s == "OLS05068")
        })
        .collect();

    assert_eq!(
        ols05068.len(),
        1,
        "Expected exactly 1 OLS05068 (for non_existing_lang), got {}: {:?}",
        ols05068.len(),
        ols05068
    );
    assert!(
        ols05068[0].message.contains("non_existing_lang"),
        "OLS05068 should be for non_existing_lang, got: {}",
        ols05068[0].message
    );
}

/// Test that changing additional_languages via config update clears/restores OLS05068 diagnostics.
#[test]
fn test_config_additional_languages_updates_diagnostics() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);

    let items_xml_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/addons/module_lang_test/data/lang_test_items.xml")
        .sanitize();

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&items_xml_path).unwrap();
    let force_republish_diagnostic = |session: &mut SessionInfo| {
        let mut file_info_borrow = file_info.borrow_mut();
        // This does not rebuild validation steps. It just sets need_push to true.
        file_info_borrow.update_validation_diagnostics(HashMap::default());
        file_info_borrow.publish_diagnostics(session);
    };

    // Verify OLS05068 is present initially (nl_WV is unknown)
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path);
    assert!(
        has_diagnostic_code(&diagnostics, "OLS05068"),
        "Initial: OLS05068 should be present for nl_WV. Diagnostics: {:?}",
        diagnostics
    );

    // Add nl_WV and nl to additional_languages via config update
    let mut new_config = session.sync_odoo.config.clone();
    new_config.set_string_list(ConfigKey::AdditionalLanguages, ["nl_WV".to_string(), "nl".to_string()]);
    Odoo::handle_config_update(&mut session, new_config, ConfigView::new());

    // OLS05068 should be gone now
    // We need to force republish diagnostics to avoid a false negative, as the
    // previous check consumes the messages.
    force_republish_diagnostic(&mut session);
    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path);
    assert!(
        !has_diagnostic_code(&diagnostics, "OLS05068"),
        "After adding nl_WV to additional_languages: OLS05068 should be CLEARED. Diagnostics: {:?}",
        diagnostics
    );

    // Restore original config (without nl_WV) and verify diagnostic reappears
    let mut restored_config = session.sync_odoo.config.clone();
    restored_config.set_string_list(ConfigKey::AdditionalLanguages, []);
    Odoo::handle_config_update(&mut session, restored_config, ConfigView::new());

    let diagnostics = get_diagnostics_for_path(&mut session, &items_xml_path);
    assert!(
        has_diagnostic_code(&diagnostics, "OLS05068"),
        "After removing nl_WV from additional_languages: OLS05068 should REAPPEAR. Diagnostics: {:?}",
        diagnostics
    );
}
