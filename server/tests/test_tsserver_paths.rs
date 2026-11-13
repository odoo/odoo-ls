//! Unit tests for generate_paths_map, that feeds tsserver's `paths`
//! Depends on the fixtures in test/data/addons

use std::path::PathBuf;

use odoo_ls_server::core::tsserver_paths::generate_paths_map;
use odoo_ls_server::utils::PathSanitizer as _;

/// Only directories with a manifest file are addons. Other directories must not
/// contribute source/test globs, library aliases, or special web declarations.
#[test]
fn non_addon_directories_are_not_mapped() {
    let temp = assert_fs::TempDir::new().unwrap();
    for name in ["valid_addon", "no_manifest", "directory_manifest", "web"] {
        let root = temp.path().join(name);
        for relative in ["static/src/@types", "static/tests", "static/lib"] {
            std::fs::create_dir_all(root.join(relative)).unwrap();
        }
        std::fs::write(root.join("static/lib/helper.js"), format!("/** @odoo-module alias=@fixture/{name} */\n")).unwrap();
        std::fs::write(root.join("static/src/@types/owl.d.ts"), "").unwrap();
        std::fs::write(root.join("static/src/@types/hoot.d.ts"), "").unwrap();
    }
    std::fs::write(temp.path().join("valid_addon/__manifest__.py"), "{}").unwrap();
    std::fs::create_dir(temp.path().join("directory_manifest/__manifest__.py")).unwrap();

    let paths = generate_paths_map(&[temp.path().to_path_buf()]);
    let valid = temp.path().join("valid_addon");
    assert_eq!(paths.get("@valid_addon/*"), Some(&vec![valid.join("static/src/*").sanitize()]));
    assert_eq!(paths.get("@valid_addon/../tests/*"), Some(&vec![valid.join("static/tests/*").sanitize()]));
    assert_eq!(paths.get("@valid_addon/../lib/helper"), Some(&vec![valid.join("static/lib/helper.js").sanitize()]));
    assert_eq!(paths.get("@fixture/valid_addon"), Some(&vec![valid.join("static/lib/helper.js").sanitize()]));
    assert_eq!(paths.len(), 4);
}

/// The fixture addons directory, the only input the map is built from.
fn addons_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/addons")
}

/// A `module_owl` path, spelled the way `generate_paths_map` writes it.
fn module_owl(relative: &str) -> Vec<String> {
    vec![addons_dir().join("module_owl").join(relative).sanitize()]
}

/// `static/src` is `@module/*` and `static/tests` is `@module/../tests/*`.
/// A folder that does not exist gets no name.
#[test]
fn glob_names_follow_the_static_folders() {
    let paths = generate_paths_map(&[addons_dir()]);
    assert_eq!(paths.get("@module_owl/*"), Some(&module_owl("static/src/*")));
    assert_eq!(paths.get("@module_owl/../tests/*"), Some(&module_owl("static/tests/*")));
    // module_1 has a `static/src` and no `static/tests`
    assert!(paths.contains_key("@module_1/*"));
    assert_eq!(paths.get("@module_1/../tests/*"), None);
    // module_csv has no `static` at all
    assert_eq!(paths.get("@module_csv/*"), None);
}

/// `static/lib` is named one file at a time, and only for the files Odoo treats as modules.
#[test]
fn lib_names_need_an_odoo_module_header() {
    let paths = generate_paths_map(&[addons_dir()]);
    assert_eq!(paths.get("@module_owl/../lib/mini/mini"), Some(&module_owl("static/lib/mini/mini.js")));
    // a directory's `index.js` is named by the directory
    assert_eq!(paths.get("@module_owl/../lib/pkg"), Some(&module_owl("static/lib/pkg/index.js")));
    // no header, so not a module, so no name
    assert_eq!(paths.get("@module_owl/../lib/bundle/bundle"), None);
    // and `static/lib` never gets the glob its siblings get
    assert_eq!(paths.get("@module_owl/../lib/*"), None);
}

/// `alias=` adds a second name. `ignore` takes every name away, the alias included.
#[test]
fn alias_adds_a_name_and_ignore_removes_them() {
    let paths = generate_paths_map(&[addons_dir()]);
    let aliased = module_owl("static/lib/aliased/aliased.js");
    assert_eq!(paths.get("@fixture/aliased"), Some(&aliased));
    assert_eq!(paths.get("@module_owl/../lib/aliased/aliased"), Some(&aliased));
    assert_eq!(paths.get("@fixture/opted_out"), None);
    assert_eq!(paths.get("@module_owl/../lib/opted_out/opted_out"), None);
}
