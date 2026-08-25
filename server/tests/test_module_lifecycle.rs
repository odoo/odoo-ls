//! Lifecycle of a module directory under `odoo/addons`.

use assert_fs::TempDir;
use assert_fs::prelude::*;
use lsp_types::{CreateFilesParams, FileCreate};
use odoo_ls_server::core::config::ConfigKey;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::core::symbols::symbol_keys::{KeyValidator, SymbolKey};
use odoo_ls_server::utils::PathSanitizer;
use std::fs;
use std::path::Path;

mod setup;

/// A directory already indexed as a namespace must become a module when its `__manifest__.py`
/// appears, and the namespace it replaces must be unloaded rather than left in the arena.
#[test]
fn test_namespace_becomes_module_when_manifest_is_created() {
    let temp = TempDir::new().unwrap();
    // `foo` is a bare directory: with no `__init__.py` and no `__manifest__.py`, the import
    // resolver can only index it as a namespace.
    temp.child("foo").create_dir_all().unwrap();
    // A module importing it, so that `odoo.addons.foo` is indexed the way production does it:
    // through the import resolver, during the ARCH of `importer`.
    temp.child("importer").create_dir_all().unwrap();
    temp.child("importer").child("__manifest__.py").write_str("{'name': 'importer'}\n").unwrap();
    temp.child("importer").child("__init__.py").write_str("from odoo.addons import foo\n").unwrap();

    // `canonicalize` so that the addons path we register and the paths we build from it are the
    // same string (on macOS the temp dir is behind a symlink).
    let addons_path = fs::canonicalize(temp.path()).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let (mut odoo, mut config) = setup::setup::setup_server(true);
    config.set_string_list(ConfigKey::AddonsPaths, [fixtures.sanitize(), addons_path.sanitize()]);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let addons = session.sync_odoo.addons_namespace().expect("odoo/addons namespace should exist");
    let child = session.st()[addons].get_child("foo");
    let Some(SymbolKey::Namespace(namespace)) = child else {
        panic!("`foo` should have been indexed as a namespace, got {child:?}");
    };

    // The manifest appears: `foo` is a module from now on.
    let manifest = addons_path.join("foo").join("__manifest__.py");
    fs::write(&manifest, "{'name': 'foo'}\n").unwrap();
    Odoo::handle_did_create(&mut session, CreateFilesParams {
        files: vec![FileCreate { uri: FileMgr::pathname2uri(&manifest.sanitize()).to_string() }],
    });

    let child = session.st()[addons].get_child("foo");
    assert!(
        matches!(child, Some(SymbolKey::Module(_))),
        "`foo` should have been replaced by a module, got {child:?}"
    );
    assert!(
        !session.st().is_key_valid(namespace),
        "the replaced namespace should have been unloaded, not left in the arena"
    );
}
