use std::path::PathBuf;

use assert_fs::TempDir;
use assert_fs::prelude::*;
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, NumberOrString, TextDocumentContentChangeEvent,
    TextDocumentItem, VersionedTextDocumentIdentifier,
};
use odoo_ls_server::S;
use odoo_ls_server::core::config::ConfigKey;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

const FIXTURE: &str = "module_duplicate_templates";

fn template_xml(t_name: &str) -> String {
    format!("<templates>\n    <t t-name=\"{t_name}\">\n        <div/>\n    </t>\n</templates>\n")
}

fn addons_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons")
}

fn has_ols05072(session: &mut SessionInfo, paths: &[String; 2]) -> [bool; 2] {
    let diagnostics = setup::setup::get_diagnostics_for_paths(session, paths);
    paths.clone().map(|path| {
        diagnostics.get(&path).is_some_and(|diags| diags.iter().any(|d| d.code == Some(NumberOrString::String(S!("OLS05072")))))
    })
}

/// OLS05072 must flag every declaration of a duplicated t-name, whatever the build order,
/// and must follow the edits of any of them.
#[test]
fn test_ols05072_duplicate_template() {
    let fixture = TempDir::new().expect("failed to create the fixture addons dir");
    let module = fixture.child(FIXTURE);
    module.child("__manifest__.py").write_str(
        "{\n    'name': 'Module Duplicate Templates',\n    'version': '1.0',\n    'depends': [],\n    'installable': True,\n}\n"
    ).unwrap();
    module.child("__init__.py").touch().unwrap();
    let duplicated = "module_duplicate_templates.Duplicated";
    module.child("static/src/a.xml").write_str(&template_xml(duplicated)).unwrap();
    module.child("static/src/b.xml").write_str(&template_xml(duplicated)).unwrap();
    let paths = [
        module.child("static/src/a.xml").path().sanitize(),
        module.child("static/src/b.xml").path().sanitize(),
    ];
    let fixture_path = fixture.path().sanitize();

    let (mut odoo, mut config) = setup::setup::setup_server(true);
    config.set_string_list(ConfigKey::AddonsPaths, [addons_path().sanitize(), fixture_path.clone()]);
    odoo.get_file_mgr().borrow_mut()
        .add_workspace_folder(S!("duplicate_templates_addons"), FileMgr::pathname2uri(&fixture_path));
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    assert_eq!(has_ols05072(&mut session, &paths), [true, true], "both declarations must be flagged");

    let uri = FileMgr::pathname2uri(&paths[0]);
    Odoo::handle_did_open(&mut session, DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "xml".to_string(),
            version: 1,
            text: template_xml(duplicated),
        },
    });
    let change = |session: &mut SessionInfo, version: i32, text: String| {
        Odoo::handle_did_change(session, DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri: uri.clone(), version },
            content_changes: vec![TextDocumentContentChangeEvent { range: None, range_length: None, text }],
        });
    };

    change(&mut session, 2, template_xml("module_duplicate_templates.Renamed"));
    assert_eq!(has_ols05072(&mut session, &paths), [false, false], "renaming one declaration must clear both");

    change(&mut session, 3, template_xml(duplicated));
    assert_eq!(has_ols05072(&mut session, &paths), [true, true], "restoring the duplicate must flag both again");
}
