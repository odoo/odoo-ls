//! `js_type_files::type_files_for` decides which of Odoo's ambient `@types` declarations tsserver
//! is handed as project roots for an opened JS file. The scope is the file's module plus its
//! manifest dependencies; the module's own docs cover why that is both necessary (the declarations
//! merge, so every one of them has to be in the same program) and sufficient (augmentation only
//! ever flows from a module towards the ones it depends on, never back down).
//!
//! No tsserver here: the scope is a pure function of the module graph and the filesystem.

use std::path::PathBuf;

use odoo_ls_server::core::js_module_scope;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::{create_init_session, setup_server};

mod setup;

fn addons_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons")
}

fn dts(module: &str, relative: &[&str]) -> String {
    let mut path = addons_path().join(module).join("static");
    for part in relative {
        path = path.join(part);
    }
    path.sanitize()
}

/// The probe path is only ever used to work out which module owns it, so it needs no content and
/// need not exist.
fn type_files_from(session: &SessionInfo, module: &str) -> Vec<String> {
    let probe = addons_path().join(module).join("static").join("src").join("probe.js").sanitize();
    js_module_scope::type_files_for(session, &probe)
}

#[test]
fn test_type_files_are_scoped_to_manifest_depends() {
    let (mut odoo, config) = setup_server(true);
    let session = create_init_session(&mut odoo, config);

    let module_1_services = dts("module_1", &["src", "@types", "services.d.ts"]);
    let module_2_models = dts("module_2", &["src", "core", "common", "@types", "models.d.ts"]);
    let vendored = dts("module_1", &["lib", "vendor", "bundle.d.ts"]);

    // module_2 depends on module_1, so both modules' declarations have to reach the same program
    // for TypeScript to merge them into one interface.
    let from_module_2 = type_files_from(&session, "module_2");
    assert!(
        from_module_2.contains(&module_1_services),
        "a dependency's declarations must be in scope, got {from_module_2:?}"
    );
    assert!(
        from_module_2.contains(&module_2_models),
        "@types nested below static/src must be found, got {from_module_2:?}"
    );

    let from_module_1 = type_files_from(&session, "module_1");
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
