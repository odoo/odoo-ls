use std::fs;
use std::path::PathBuf;

use assert_fs::TempDir;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileCreate, FileDelete, FileRename, RenameFilesParams,
    TextDocumentContentChangeEvent,
};
use odoo_ls_server::constants::OYarn;
use odoo_ls_server::core::build_scheduler::BuildScheduler;
use odoo_ls_server::core::config::ConfigKey;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::core::symbols::symbol_keys::ModuleKey;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use odoo_ls_server::{S, Sy};

mod setup;

/// One shared setup.
/// `test_js_lib_file_no_oxc_diagnostics` drains the session channel, so it runs last.
#[test]
fn test_js_lifecycle() {
    let fixture = build_fixture_addons();
    let fixture_path = fixture.path().sanitize();

    let (mut odoo, mut config) = setup::setup::setup_server(true);
    config.set_string_list(ConfigKey::AddonsPaths, [addons_path().sanitize(), fixture_path.clone()]);
    odoo.get_file_mgr().borrow_mut()
        .add_workspace_folder(S!("asset_events_addons"), FileMgr::pathname2uri(&fixture_path));
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    test_asset_events(&mut session, &fixture);
    test_js_file_lifecycle_with_odoo(&mut session);
    test_js_lib_file_no_oxc_diagnostics(&mut session);
}

fn make_js_open_params(uri: lsp_types::Uri, content: &str) -> lsp_types::DidOpenTextDocumentParams {
    lsp_types::DidOpenTextDocumentParams {
        text_document: lsp_types::TextDocumentItem {
            uri,
            language_id: "javascript".to_string(),
            version: 1,
            text: content.to_string(),
        },
    }
}

fn make_js_change_params(uri: lsp_types::Uri, version: i32, content: &str) -> lsp_types::DidChangeTextDocumentParams {
    lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri, version },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: content.to_string(),
        }],
    }
}

fn make_js_close_params(uri: lsp_types::Uri) -> lsp_types::DidCloseTextDocumentParams {
    lsp_types::DidCloseTextDocumentParams {
        text_document: lsp_types::TextDocumentIdentifier { uri },
    }
}

// — asset discovery from watched file events —

const FIXTURE: &str = "module_asset_events";

/// A second addons root we can makes changes on
fn build_fixture_addons() -> TempDir {
    let fixture = TempDir::new().expect("failed to create the fixture addons dir");
    let module = fixture.child(FIXTURE);
    // No `assets` key: discovery must not need one.
    module.child("__manifest__.py").write_str(
        "{\n    'name': 'Module Asset Events',\n    'version': '1.0',\n    'depends': [],\n    'installable': True,\n}\n"
    ).unwrap();
    module.child("__init__.py").touch().unwrap();
    module.child("static/src/existing.js").write_str("export const existingHelper = () => 1;\n").unwrap();
    fixture
}

fn addons_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons")
}

fn fixture_module(session: &SessionInfo) -> ModuleKey {
    session.sync_odoo.modules.get(&Sy!(FIXTURE))
        .expect("module_asset_events not loaded")
        .upgrade(session.st())
        .expect("module_asset_events symbol is dead")
}

fn has_js_symbol(session: &SessionInfo, path: &str) -> bool {
    session.st()[fixture_module(session)].js_symbols().contains_key(path)
}

fn has_data_symbol(session: &SessionInfo, path: &str) -> bool {
    session.st()[fixture_module(session)].data_file_symbols().contains_key(path)
}

fn has_custom_entry(session: &SessionInfo, path: &str) -> bool {
    session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path == path)
}

/// Writes `relative` under the fixture module and gives back the path the symbol maps are keyed by.
fn write_asset(module: &ChildPath, relative: &str, content: &str) -> String {
    let file = module.child(relative);
    file.write_str(content).expect("failed to write fixture file");
    file.path().sanitize()
}

fn did_create(session: &mut SessionInfo, path: &str) {
    Odoo::handle_did_create(session, CreateFilesParams {
        files: vec![FileCreate { uri: FileMgr::pathname2uri(path).to_string() }],
    });
}

fn did_rename(session: &mut SessionInfo, old_path: &str, new_path: &str) {
    Odoo::handle_did_rename(session, RenameFilesParams {
        files: vec![FileRename {
            old_uri: FileMgr::pathname2uri(old_path).to_string(),
            new_uri: FileMgr::pathname2uri(new_path).to_string(),
        }],
    });
}

fn did_delete(session: &mut SessionInfo, path: &str) {
    Odoo::handle_did_delete(session, DeleteFilesParams {
        files: vec![FileDelete { uri: FileMgr::pathname2uri(path).to_string() }],
    });
}

/// Assets are discovered by walking `static/{src,tests,lib}`, never from the manifest — the
/// fixture module declares no `assets` at all. These bodies drive the watched-file events that
/// have to keep that walk up to date.
fn test_asset_events(session: &mut SessionInfo, fixture: &TempDir) {
    let module = fixture.child(FIXTURE);
    let src_dir = module.path().join("static").join("src");
    let sub_dir = src_dir.join("sub");
    let renamed_sub_dir = src_dir.join("sub2");

    // 1. a new file under static/src joins the module, instead of getting an entry point of its own
    let created = write_asset(&module, "static/src/created.js", "export const created = () => 1;\n");
    did_create(session, &created);
    assert!(has_js_symbol(session, &created), "a created static/src JS file must join the module");
    assert!(!has_custom_entry(session, &created), "a module asset must not get a custom entry point");

    // 2. a didOpen that beats the create event must reach the same state
    let opened_content = "export const opened = () => 2;\n";
    let opened = write_asset(&module, "static/src/opened.js", opened_content);
    let opened_uri = FileMgr::pathname2uri(&opened);
    Odoo::handle_did_open(session, make_js_open_params(opened_uri.clone(), opened_content));
    assert!(has_js_symbol(session, &opened), "didOpen alone must load a new asset into its module");
    assert!(!has_custom_entry(session, &opened), "didOpen must not race the create event to a custom entry point");
    Odoo::handle_did_close(session, make_js_close_params(opened_uri));

    // 3. XML too. Driven through `on_new_path` alone, without the `process_rebuilds`
    // that follows it in `handle_did_create`: nothing depends on a file that did not exist, so the
    // hook is the only thing that can have loaded it.
    let created_xml = write_asset(
        &module,
        "static/src/created.xml",
        "<templates xml:space=\"preserve\">\n    <t t-name=\"module_asset_events.Created\">\n        <div/>\n    </t>\n</templates>\n",
    );
    Odoo::on_new_path(session, &created_xml);
    assert!(has_data_symbol(session, &created_xml), "a created static/src XML asset must be loaded by the hook, not by a module rebuild");
    BuildScheduler::process_rebuilds(session, false);

    // 4. in static/lib the @odoo-module header is what makes a file an asset
    let plain_lib = write_asset(&module, "static/lib/plain.js", "window.plain = 1;\n");
    did_create(session, &plain_lib);
    assert!(!has_js_symbol(session, &plain_lib), "a headerless static/lib file is not an asset");

    // 5.
    let headed_lib = write_asset(&module, "static/lib/headed.js", "/** @odoo-module */\nexport const headed = () => 5;\n");
    did_create(session, &headed_lib);
    assert!(has_js_symbol(session, &headed_lib), "a headed static/lib file is an asset");

    // 6. only src, tests and lib hold assets
    let outside = write_asset(&module, "static/description/ignored.js", "export const ignored = () => 6;\n");
    did_create(session, &outside);
    assert!(!has_js_symbol(session, &outside), "static/description is not an asset folder");

    // 7. file rename
    let renamed = src_dir.join("renamed.js").sanitize();
    fs::rename(&created, &renamed).expect("failed to rename the JS file");
    did_rename(session, &created, &renamed);
    assert!(!has_js_symbol(session, &created), "the old path must be gone after a rename");
    assert!(has_js_symbol(session, &renamed), "the new path must be loaded after a rename");

    // 8. a folder rename arrives as ONE event naming the folder, never its children
    let nested = write_asset(&module, "static/src/sub/nested.js", "export const nested = () => 8;\n");
    let nested_xml = write_asset(
        &module,
        "static/src/sub/nested.xml",
        "<templates xml:space=\"preserve\">\n    <t t-name=\"module_asset_events.Nested\">\n        <div/>\n    </t>\n</templates>\n",
    );
    did_create(session, &nested);
    did_create(session, &nested_xml);
    assert!(has_js_symbol(session, &nested));
    assert!(has_data_symbol(session, &nested_xml));

    fs::rename(&sub_dir, &renamed_sub_dir).expect("failed to rename the asset folder");
    did_rename(session, &sub_dir.sanitize(), &renamed_sub_dir.sanitize());

    let moved_js = renamed_sub_dir.join("nested.js").sanitize();
    let moved_xml = renamed_sub_dir.join("nested.xml").sanitize();
    assert!(!has_js_symbol(session, &nested), "every JS file under the old folder must be gone");
    assert!(!has_data_symbol(session, &nested_xml), "every XML file under the old folder must be gone");
    assert!(has_js_symbol(session, &moved_js), "every JS file under the new folder must be loaded");
    assert!(has_data_symbol(session, &moved_xml), "every XML file under the new folder must be loaded");

    // 9. a folder delete arrives the same way
    fs::remove_dir_all(&renamed_sub_dir).expect("failed to delete the asset folder");
    did_delete(session, &renamed_sub_dir.sanitize());
    assert!(!has_js_symbol(session, &moved_js), "a folder delete must drop the JS files under it");
    assert!(!has_data_symbol(session, &moved_xml), "a folder delete must drop the XML files under it");
}

/// Full JS file lifecycle: open → edit → close → rename → reopen.
fn test_js_file_lifecycle_with_odoo(session: &mut SessionInfo) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let js_file = temp_dir.child("test_component.js");
    let initial_content = "/** @odoo-module */\nexport class TestComponent {}\n";
    js_file.write_str(initial_content).expect("Failed to write JS file");

    let js_path = js_file.path().sanitize();
    let js_uri = FileMgr::pathname2uri(&js_path);

    // — open —
    Odoo::handle_did_open(session, make_js_open_params(js_uri.clone(), initial_content));

    assert!(
        session.sync_odoo.opened_files.contains(&js_path),
        "JS file should be in opened_files after didOpen"
    );
    assert!(
        session.sync_odoo.get_file_mgr().borrow().get_file_info(&js_path).is_some(),
        "FileInfo should exist after didOpen"
    );
    let has_entry = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path.contains("test_component"));
    assert!(has_entry, "Custom entry point should be created for the JS file");

    // — edit —
    let edited_content = "/** @odoo-module */\nexport class TestComponent { setup() {} }\n";
    Odoo::handle_did_change(session, make_js_change_params(js_uri.clone(), 2, edited_content));

    assert!(
        session.sync_odoo.opened_files.contains(&js_path),
        "JS file should still be in opened_files after didChange"
    );
    {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let file_mgr = file_mgr.borrow();
        let file_info = file_mgr.get_file_info(&js_path).expect("FileInfo must exist after edit");
        assert_eq!(
            file_info.borrow().version,
            Some(2),
            "File version should be updated to 2 after didChange"
        );
    }

    // — close —
    Odoo::handle_did_close(session, make_js_close_params(js_uri.clone()));

    assert!(
        !session.sync_odoo.opened_files.contains(&js_path),
        "JS file should be removed from opened_files after didClose"
    );
    let entry_after_close = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path.contains("test_component") && !ep.borrow().path.contains("renamed"));
    assert!(!entry_after_close, "Custom entry for original JS file should be removed after didClose");

    // — rename on disk —
    let new_js_file = temp_dir.child("test_component_renamed.js");
    fs::rename(js_file.path(), new_js_file.path()).expect("Failed to rename JS file on disk");

    let new_js_path = new_js_file.path().sanitize();
    let new_js_uri = FileMgr::pathname2uri(&new_js_path);

    Odoo::handle_did_rename(session, lsp_types::RenameFilesParams {
        files: vec![lsp_types::FileRename {
            old_uri: js_uri.to_string(),
            new_uri: new_js_uri.to_string(),
        }],
    });

    assert!(
        !session.sync_odoo.opened_files.contains(&js_path),
        "Old path should not be in opened_files after rename"
    );

    // — open renamed file —
    Odoo::handle_did_open(session, make_js_open_params(new_js_uri.clone(), edited_content));

    assert!(
        session.sync_odoo.opened_files.contains(&new_js_path),
        "Renamed JS file should be in opened_files after re-open"
    );
    assert!(
        session.sync_odoo.get_file_mgr().borrow().get_file_info(&new_js_path).is_some(),
        "FileInfo should exist for renamed JS file"
    );
    let has_renamed_entry = session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter()
        .any(|ep| ep.borrow().path.contains("test_component_renamed"));
    assert!(has_renamed_entry, "Custom entry point should exist for renamed JS file");

    // — edit renamed file —
    let final_content = "/** @odoo-module */\nexport class TestComponent { setup() {} destroy() {} }\n";
    Odoo::handle_did_change(session, make_js_change_params(new_js_uri.clone(), 2, final_content));
    {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let file_mgr = file_mgr.borrow();
        let file_info = file_mgr.get_file_info(&new_js_path).expect("FileInfo must exist after edit of renamed file");
        assert_eq!(
            file_info.borrow().version,
            Some(2),
            "Renamed file version should be 2 after edit"
        );
    }

    // — close renamed file —
    Odoo::handle_did_close(session, make_js_close_params(new_js_uri.clone()));

    assert!(
        !session.sync_odoo.opened_files.contains(&new_js_path),
        "Renamed JS file should be removed from opened_files after final close"
    );

}

/// A JS file inside a static/lib/ path should not get OXC diagnostics.
/// Ensures the is_lib exclusion works end-to-end.
fn test_js_lib_file_no_oxc_diagnostics(session: &mut SessionInfo) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let lib_js = temp_dir.child("static/lib/external_lib.js");
    // Deliberately invalid JS — OXC would flag it if not excluded
    let bad_js = "this is not valid javascript @@@;\n";
    lib_js.write_str(bad_js).expect("Failed to write lib JS");

    let lib_path = lib_js.path().sanitize();
    let lib_uri = FileMgr::pathname2uri(&lib_path);

    Odoo::handle_did_open(session, make_js_open_params(lib_uri.clone(), bad_js));

    let diagnostics = setup::setup::get_diagnostics_for_path(session, &lib_path);
    let oxc_diagnostics: Vec<_> = diagnostics.iter().filter(|d| {
        matches!(&d.source, Some(s) if s.contains("oxc") || s.contains("OXC"))
    }).collect();
    assert!(
        oxc_diagnostics.is_empty(),
        "Files under static/lib/ should not receive OXC diagnostics, got: {:?}",
        oxc_diagnostics
    );

    Odoo::handle_did_close(session, make_js_close_params(lib_uri));
}
