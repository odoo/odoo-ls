//! What an Odoo module's JS may legitimately reach: the module owning a file, plus that module's
//! manifest dependency closure. Odoo loads only declared dependencies, so an import naming
//! anything outside the closure fails at runtime.
//!
//! tsserver sees only its *program* — the roots we hand it plus whatever those transitively
//! import. Anything it must reason about therefore has to be named as a root, and two sets never
//! get there on their own:
//!
//! - [`type_files_for`] — Odoo's ambient declarations (`declare module "plugins"`, `"services"`,
//!   `"models"`, …) under `<module>/static/**/@types/`. Nothing imports them by path, and modules
//!   redeclare the same name on purpose to extend it through interface merging (`"models"` over
//!   ~36 files), which again only covers the program. Hence `typeRoots` is sent **empty but
//!   present**: absent falls back to every ancestor's `node_modules/@types` (jQuery, luxon and
//!   qunit in every file's global scope), filled resolves a bare `import("plugins")`
//!   first-match-wins and silently drops the rest.
//! - [`importable_files_for`] — the closure's importable JS. A symbol nothing has imported yet is
//!   outside the program, so on a cold session auto-import has almost nothing to offer.
//!
//! Both unions only ever grow: closures overlap, so no per-file contribution can be subtracted.
//!
//! Scoping to the closure is correct on its own terms — augmentation flows *upward*, html_editor
//! knows nothing of website — and far cheaper than the everything-at-once program a
//! `jsconfig.json` glob would build. The program is still one global set whose export map offers
//! every symbol from every file, so [`importable_module_prefixes`] narrows the offered entries
//! back down to the editing file's own closure.


use std::ffi::OsStr;
use std::path::Path;

use crate::core::file_mgr::{Ast, FileMgr};
use crate::core::symbols::symbol_keys::ModuleKey;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;

/// Every `.d.ts` shipped by the Odoo module owning `path` and by its manifest dependency closure.
pub fn type_files_for(session: &SessionInfo, path: &str) -> Vec<String> {
    let Some(module_key) = module_of_path(session, path) else {
        return vec![];
    };
    let mut files = vec![];
    for dep in dependency_closure(session, module_key) {
        collect_type_files(&Path::new(&session.st()[dep].path).join("static"), &mut files);
    }
    files
}

/// The JS files tsserver should carry as project roots so that a file in `path`'s module can
/// auto-import from everywhere it is allowed to.
pub fn importable_files_for(session: &SessionInfo, path: &str) -> Vec<String> {
    let Some(module) = module_of_path(session, path) else {
        return vec![];
    };
    let mut files = vec![];
    let deps = dependency_closure(session, module);
    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_mgr = file_mgr.borrow();
    for dep in deps {
        files.extend(
            session.st()[dep].js_symbols().keys()
                // skipping files that have nothing to import from helps us keep the program small
                .filter(|path| js_has_exports(&file_mgr, path).unwrap_or(true))
                .cloned(),
        );
    }
    files
}

/// `module` itself followed by its transitive manifest dependencies. Dependencies naming a module
/// that is not loaded are dropped.
fn dependency_closure(session: &SessionInfo, module: ModuleKey) -> Vec<ModuleKey> {
    let symbol = &session.st()[module];
    // `all_depends` is already the transitive closure, and excludes the module itself.
    std::iter::once(&symbol.dir_name)
        .chain(symbol.get_all_depends())
        .filter_map(|dir_name| session.sync_odoo.modules.get(dir_name)?.upgrade(session.st()))
        .collect()
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

/// `None` when the file is not cached, or is not JS.
fn js_has_exports(file_mgr: &FileMgr, path: &str) -> Option<bool> {
    let file_info = file_mgr.get_file_info(path)?;
    match &file_info.borrow().file_info_ast.borrow().ast {
        Ast::JsAst(js_ast) => Some(js_ast.has_exports),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn an_uncached_file_gives_no_answer() {
        // The filter in `importable_files_for` reads `None` as "keep it": dropping
        // a file we know nothing about would silently cost an import
        // suggestion.
        assert_eq!(js_has_exports(&FileMgr::new(), "/not/cached.js"), None);
    }
}
