use std::fs;

use assert_fs::TempDir;
use assert_fs::prelude::*;
use odoo_ls_server::core::config::ConfigKey;
use odoo_ls_server::core::dependency_report::generate_report;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

/// End-to-end smoke test for the `--list-python-dependencies` report generator: builds a small,
/// self-contained pair of modules exercising every classification the report makes, then checks
/// the resulting JSON shape.
#[test]
fn test_generate_report_classifies_every_dependency_kind() {
    let temp = TempDir::new().unwrap();

    let mod_b = temp.child("mod_b");
    mod_b.create_dir_all().unwrap();
    mod_b.child("__manifest__.py").write_str("{'name': 'mod_b', 'depends': ['base']}\n").unwrap();
    mod_b.child("__init__.py").write_str("").unwrap();

    // mod_d -> mod_c -> base: mod_c is only reachable *transitively*, through mod_d.
    let mod_c = temp.child("mod_c");
    mod_c.create_dir_all().unwrap();
    mod_c.child("__manifest__.py").write_str("{'name': 'mod_c', 'depends': ['base']}\n").unwrap();
    mod_c.child("__init__.py").write_str("").unwrap();

    let mod_d = temp.child("mod_d");
    mod_d.create_dir_all().unwrap();
    mod_d.child("__manifest__.py").write_str("{'name': 'mod_d', 'depends': ['mod_c']}\n").unwrap();
    mod_d.child("__init__.py").write_str("").unwrap();

    let mod_a = temp.child("mod_a");
    mod_a.create_dir_all().unwrap();
    // 'mod_b' is used but NOT declared anywhere: a genuinely missing dependency.
    // 'mod_c' is used but only declared transitively (through 'mod_d', which IS declared): indirect.
    // 'mod_d' and 'base' are declared directly.
    mod_a.child("__manifest__.py").write_str("{'name': 'mod_a', 'depends': ['base', 'mod_d']}\n").unwrap();
    mod_a.child("__init__.py").write_str(
        r#"
from odoo import fields
import os
from odoo.addons import mod_b
from odoo.addons import mod_c
from odoo.addons import mod_d
import xlsxwriter

try:
    import ujson
except ImportError:
    import json as ujson
"#,
    ).unwrap();

    let addons_path = fs::canonicalize(temp.path()).unwrap();
    let (mut odoo, mut config) = setup::setup::setup_server(true);
    config.set_string_list(ConfigKey::AddonsPaths, [addons_path.sanitize()]);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let report = generate_report(&mut session);
    let mod_a_report = &report["modules"]["mod_a"];
    let dependencies = mod_a_report["dependencies"].as_array().expect("dependencies should be an array");

    let find = |name: &str| dependencies.iter().find(|d| d["name"] == name);

    // 'odoo' core (import odoo/from odoo import ...) and stdlib ('os', 'json') must never appear.
    assert!(find("odoo").is_none(), "odoo core framework must be excluded, got: {:?}", dependencies);
    assert!(find("os").is_none(), "stdlib imports must be excluded, got: {:?}", dependencies);
    assert!(find("json").is_none(), "stdlib imports must be excluded even inside except handlers, got: {:?}", dependencies);

    // mod_b (not reachable at all) and mod_c (reachable only through mod_d) are both flagged
    // as needing action; whether a used-but-undeclared module is at least reachable
    // transitively is visible in `manifest_depends` instead (checked below).
    let mod_b_dep = find("mod_b").expect("mod_b should be reported as a dependency");
    assert_eq!(mod_b_dep["type"], "odoo_module");
    assert_eq!(mod_b_dep["in_manifest"], false);
    assert_eq!(mod_b_dep["action_needed"], true);

    let mod_c_dep = find("mod_c").expect("mod_c should be reported as a dependency");
    assert_eq!(mod_c_dep["type"], "odoo_module");
    assert_eq!(mod_c_dep["in_manifest"], false);
    assert_eq!(mod_c_dep["action_needed"], true);

    let mod_d_dep = find("mod_d").expect("mod_d should be reported as a dependency");
    assert_eq!(mod_d_dep["type"], "odoo_module");
    assert_eq!(mod_d_dep["in_manifest"], true);
    assert_eq!(mod_d_dep["action_needed"], false);

    let xlsxwriter_dep = find("xlsxwriter").expect("xlsxwriter should be reported as a dependency");
    assert_eq!(xlsxwriter_dep["type"], "python_module");
    assert_eq!(xlsxwriter_dep["in_manifest"], false);
    assert_eq!(xlsxwriter_dep["action_needed"], true);
    let xlsxwriter_occurrences = xlsxwriter_dep["occurrences"].as_array().unwrap();
    assert_eq!(xlsxwriter_occurrences[0]["import_context"], "direct");

    let ujson_dep = find("ujson").expect("ujson should be reported as a dependency");
    assert_eq!(ujson_dep["type"], "python_module");
    let ujson_occurrences = ujson_dep["occurrences"].as_array().unwrap();
    assert_eq!(ujson_occurrences[0]["import_context"], "try");

    // manifest_depends carries the full transitive closure (base, mod_d, and mod_d's own
    // dependency mod_c), each tagged with whether mod_a declares it directly.
    let manifest_depends = mod_a_report["manifest_depends"].as_array().unwrap();
    let find_manifest_dep = |name: &str| manifest_depends.iter().find(|d| d["name"] == name);

    assert_eq!(find_manifest_dep("base").expect("base")["direct"], true);
    assert_eq!(find_manifest_dep("mod_d").expect("mod_d")["direct"], true);
    assert_eq!(find_manifest_dep("mod_c").expect("mod_c, reachable transitively through mod_d")["direct"], false);
    assert!(find_manifest_dep("mod_b").is_none(), "mod_b is not a dependency of mod_a at all, direct or transitive");
}
