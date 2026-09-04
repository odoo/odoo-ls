//! Tests for public functions in js_module_scope
//! No tsserver here: the scope is a pure function of the module graph and the asset bundles.

use std::path::PathBuf;
use odoo_ls_server::core::js_module_scope;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;
use crate::setup::setup::{create_init_session, setup_server};
mod setup;

#[test]
fn test_js_module_scope() {
    let (mut odoo, config) = setup_server(true);
    let session = create_init_session(&mut odoo, config);
    
    test_type_files_are_scoped_to_manifest_depends(&session);
    test_importable_files_are_scoped_to_manifest_depends(&session);
    test_only_importable_files_are_roots(&session);
    test_importable_prefixes_cover_the_same_closure(&session);
}

// Shared helpers

fn addons_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons")
}

fn abs_path(module: &str, relative: &[&str]) -> String {
    let mut path = addons_path().join(module).join("static");
    for part in relative {
        path = path.join(part);
    }
    path.sanitize()
}

/// The probe path is only ever used to work out which module owns it, so it needs no content and
/// need not exist.
fn probe(module: &str) -> String {
    addons_path().join(module).join("static").join("src").join("probe.js").sanitize()
}


// Tests

/// `js_module_scope::type_files_for` decides which of Odoo's ambient `@types` declarations tsserver
/// is handed as project roots for an opened JS file. The scope is the file's module plus its
/// manifest dependencies.
fn test_type_files_are_scoped_to_manifest_depends(session: &SessionInfo) {
    // helper
    fn type_files_for(session: &SessionInfo, module: &str) -> Vec<String> {
        js_module_scope::type_files_for(session, &probe(module))
    }
    
    let module_1_services = abs_path("module_1", &["src", "@types", "services.d.ts"]);
    let module_2_models = abs_path("module_2", &["src", "core", "common", "@types", "models.d.ts"]);
    let vendored = abs_path("module_1", &["lib", "vendor", "bundle.d.ts"]);

    // module_2 depends on module_1, so both modules' declarations have to reach the same program
    // for TypeScript to merge them into one interface.
    let from_module_2 = type_files_for(&session, "module_2");
    assert!(
        from_module_2.contains(&module_1_services),
        "a dependency's declarations must be in scope, got {from_module_2:?}"
    );
    assert!(
        from_module_2.contains(&module_2_models),
        "@types nested below static/src must be found, got {from_module_2:?}"
    );

    let from_module_1 = type_files_for(&session, "module_1");
    assert!(
        from_module_1.contains(&module_1_services),
        "a module's own declarations must be in scope, got {from_module_1:?}"
    );
    // module_1 does not depend on module_2: its code can never refer to what module_2 declares, so
    // pulling it in would widen completions and grow the program for nothing.
    assert!(
        !from_module_1.contains(&module_2_models),
        "a dependent's declarations must not leak downward, got {from_module_1:?}"
    );

    // Vendored bundles are pruned, matching the excludes in Odoo's own jsconfig.
    assert!(
        !from_module_2.contains(&vendored),
        "static/lib must be pruned, got {from_module_2:?}"
    );
}

fn test_importable_files_are_scoped_to_manifest_depends(session: &SessionInfo) {
    // helper
    fn importable_files_for(session: &SessionInfo, module: &str) -> Vec<String> {
        js_module_scope::importable_files_for(session, &probe(module))
    }
    
    let module_1_shared = abs_path("module_1", &["src", "scoped", "shared.js"]);
    let module_2_local = abs_path("module_2", &["src", "scoped", "local.js"]);

    let from_module_2 = importable_files_for(&session, "module_2");
    assert!(
        from_module_2.contains(&module_2_local),
        "a module's own bundled JS must be a root, got {from_module_2:?}"
    );
    // Without this, nothing in module_1 can be auto-imported until some open file happens to
    // import it: tsserver's export map only walks the program.
    assert!(
        from_module_2.contains(&module_1_shared),
        "a dependency's bundled JS must be a root, got {from_module_2:?}"
    );
    let from_module_1 = importable_files_for(&session, "module_1");
    assert!(from_module_1.contains(&module_1_shared));
    assert!(
        !from_module_1.contains(&module_2_local),
        "a dependent's JS must not leak downward, got {from_module_1:?}"
    );
}

/// Which *files* of those modules are importable and should be included as
/// roots in the tsserver program. A root only earns its place if an import
/// could name something in it, and `has_exports` answers that: Odoo registers a
/// `static/lib` file only when it carries a header, and `parse_js_inner` skips
/// parsing the ones it does not, so their `has_exports` stays false.
fn test_only_importable_files_are_roots(session: &SessionInfo) {
    let importable = js_module_scope::importable_files_for(session, &probe("module_1"));

    let headed = abs_path("module_1", &["lib", "headed", "headed.js"]);
    assert!(
        importable.contains(&headed),
        "a static/lib file with a header is importable and must be a root, got {importable:?}"
    );

    // Both fixtures export something, so these two assertions fail if the header rule stops
    // being applied.
    let headerless = abs_path("module_1", &["lib", "vendor", "bundle.js"]);
    assert!(
        !importable.contains(&headerless),
        "a headerless static/lib bundle must not be a root, got {importable:?}"
    );
    let opted_out = abs_path("module_1", &["lib", "opted_out", "opted_out.js"]);
    assert!(
        !importable.contains(&opted_out),
        "an `ignore`d static/lib file must not be a root, got {importable:?}"
    );

    // A patch file is a module, but it offers nothing to import.
    let side_effect = abs_path("module_1", &["src", "scoped", "side_effect.js"]);
    assert!(
        !importable.contains(&side_effect),
        "a file that exports nothing must not be a root, got {importable:?}"
    );
}

fn test_importable_prefixes_cover_the_same_closure(session: &SessionInfo) {
    let module_1_dir = format!("{}/", addons_path().join("module_1").sanitize());
    let module_2_dir = format!("{}/", addons_path().join("module_2").sanitize());

    let from_module_2 = js_module_scope::importable_module_prefixes(&session, &probe("module_2"))
        .expect("a file under a module's static/ belongs to that module");
    assert!(from_module_2.contains(&module_1_dir), "got {from_module_2:?}");
    assert!(from_module_2.contains(&module_2_dir), "got {from_module_2:?}");

    // The program is global: module_2's files reach it, so its exports are offered everywhere
    // unless this filter says otherwise.
    let from_module_1 = js_module_scope::importable_module_prefixes(&session, &probe("module_1"))
        .expect("a file under a module's static/ belongs to that module");
    assert!(from_module_1.contains(&module_1_dir), "got {from_module_1:?}");
    assert!(!from_module_1.contains(&module_2_dir), "got {from_module_1:?}");

    // A file outside every module has no closure to be filtered against.
    let outside = addons_path().join("not_a_module").join("elsewhere.js").sanitize();
    assert!(js_module_scope::importable_module_prefixes(&session, &outside).is_none());
}
