//! The `.d.ts` files tsserver must carry as project roots for a given JS/TS file.
//!
//! Odoo declares its shared types as *ambient modules* — `declare module "plugins"`, `"services"`,
//! `"models"`, `"registries"` — under `<module>/static/**/@types/`. Several modules declare the
//! same name on purpose and rely on TypeScript interface merging to extend it: `SharedMethods` is
//! spread over html_editor, html_builder, website and website_blog, and `"models"` over ~36 files.
//! Merging only covers files that are in tsserver's program, and a file only gets there by being a
//! root or being imported from one — nothing ever imports an ambient `.d.ts` by path. Left alone
//! tsserver resolves a bare `import("plugins")` through `typeRoots`, first-match-wins: one
//! arbitrary module's file is elected and the rest are dropped silently. So the project sends
//! `typeRoots` **empty but present** — absent is not the same thing, it falls back to every
//! ancestor's `node_modules/@types` and puts jQuery/luxon/qunit in every file's global scope — and
//! the declarations are named as roots instead.
//!
//! The set is scoped to the file's module and its manifest dependencies. Augmentation flows
//! *upward*: html_editor knows nothing of website, website depends on html_editor. A module can
//! therefore only legitimately see its own dependency closure's declarations — which is both
//! narrower than the whole workspace and closer to what the code may actually use than the
//! everything-at-once program a `jsconfig.json` glob would build.
//!
//! The staged union is never subtracted from, because closures overlap, so there is no per-file
//! contribution to remove.

use std::ffi::OsStr;
use std::path::Path;

use crate::core::symbols::symbol_keys::ModuleKey;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;

/// Every `.d.ts` shipped by the Odoo module owning `path` and by its manifest dependency closure.
pub fn type_files_for(session: &SessionInfo, path: &str) -> Vec<String> {
    let Some(module_key) = module_of_path(session, path) else {
        return vec![];
    };
    let module = &session.st()[module_key];
    let mut files = vec![];
    // `all_depends` is already the transitive closure, and excludes the module itself.
    for dir_name in std::iter::once(&module.dir_name).chain(module.get_all_depends()) {
        let Some(dep) = session.sync_odoo.modules.get(dir_name).and_then(|dep| dep.upgrade(session.st())) else {
            continue;
        };
        collect_type_files(&Path::new(&session.st()[dep].path).join("static"), &mut files);
    }
    files
}

/// The module owning `path`. JS lives at `<module>/static/…`
fn module_of_path(session: &SessionInfo, path: &str) -> Option<ModuleKey> {
    let mut dir = Path::new(path).parent()?;
    loop {
        if dir.file_name() == Some(OsStr::new("static"))
            && let Some(module) = dir.parent().and_then(|parent| module_at(session, parent))
        {
            return Some(module);
        }
        dir = dir.parent()?;
    }
}

/// The module registered at exactly `dir`. 
fn module_at(session: &SessionInfo, dir: &Path) -> Option<ModuleKey> {
    let module = session.sync_odoo.modules.get(dir.file_name()?.to_str()?)?.upgrade(session.st())?;
    // Definite check
    (Path::new(&session.st()[module].path) == dir).then_some(module)
}

/// Recursive: the `@types` dirs sit at ~22 different depths, so nothing shallower finds them all.
/// `lib/` and `node_modules/` are pruned — vendored bundles, excluded by Odoo's jsconfig too.
fn collect_type_files(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().map(|file_type| file_type.is_dir()).unwrap_or(false) {
            if !matches!(entry.file_name().to_str(), Some("lib" | "node_modules")) {
                collect_type_files(&path, out);
            }
        } else if path.to_str().is_some_and(|path| path.ends_with(".d.ts")) {
            out.push(path.sanitize());
        }
    }
}
