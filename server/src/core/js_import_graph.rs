//! Reverse import graph over the workspace's JS files, used to make find-references
//! complete: tsserver's program only grows *forward* from its roots, so the files that
//! could reference a symbol must be named as roots before the query. Instead of all
//! transitive importers, the roots are the *exposer* closure (re-export and subclass edges
//! only) plus their direct importers — see `server/docs/owl-virtual-docs.md` §5 for the
//! rationale, the measured numbers and the known gaps.
//!
//! Edges come free from OXC's module records (`FileInfoAst::{js_imports, js_reexports}`);
//! the graph is rebuilt on demand (~15k edges in milliseconds), so nothing can go stale.

use std::path::{Component, Path, PathBuf};

use crate::core::file_mgr::Ast;
use crate::threads::SessionInfo;
use crate::utils::{HashMap, HashSet, PathSanitizer};

/// Which JS files import (or re-export from) which other JS files. Both maps are keyed by the
/// **imported** file and valued by the files pointing at it, i.e. they are already reversed.
struct ImportGraph {
    /// imported file → files that import it directly (any kind of import).
    importers: HashMap<String, HashSet<String>>,
    /// imported file → files that re-export from it (`export … from`, `export * from`).
    reexporters: HashMap<String, HashSet<String>>,
}

impl ImportGraph {
    /// Scan every parsed JS file and resolve its recorded specifiers against the workspace.
    fn build(session: &SessionInfo) -> Self {
        let js_files: Vec<(String, Vec<String>, Vec<String>)> = session
            .sync_odoo
            .get_file_mgr()
            .borrow()
            .files
            .values()
            .filter_map(|file_info| {
                let file_info = file_info.borrow();
                let ast = file_info.file_info_ast.borrow();
                matches!(ast.ast, Ast::JsAst(_)).then(|| {
                    // TODO: review these expensive clones
                    (file_info.uri.clone(), ast.ast.as_js_ast().js_imports.clone(), ast.ast.as_js_ast().js_reexports.clone())
                })
            })
            .collect();

        let known: HashSet<String> = js_files.iter().map(|(path, _, _)| path.clone()).collect();
        let module_src = module_src_dirs(session);

        let mut importers: HashMap<String, HashSet<String>> = HashMap::default();
        let mut reexporters: HashMap<String, HashSet<String>> = HashMap::default();
        for (path, imports, reexports) in js_files.iter() {
            let reexported: HashSet<&str> = reexports.iter().map(String::as_str).collect();
            for specifier in imports {
                let Some(target) = resolve_specifier(specifier, path, &module_src, &known) else {
                    continue; // npm package, `/static/lib/` file, or an alias we do not model
                };
                if reexported.contains(specifier.as_str()) {
                    reexporters.entry(target.clone()).or_default().insert(path.clone());
                }
                importers.entry(target).or_default().insert(path.clone());
            }
        }
        ImportGraph { importers, reexporters }
    }
}

/// Directory name → `static/src` directory, mirroring the `@{module}/*` alias map handed
/// to tsserver (whose resolution ours must agree with). The alias is built from the module
/// *directory* name, hence keyed on `dir_name`.
fn module_src_dirs(session: &SessionInfo) -> HashMap<String, String> {
    session
        .sync_odoo
        .modules
        .values()
        .filter_map(|module| {
            let module = module.upgrade(session.st())?;
            let module = &session.st()[module];
            let src = PathBuf::from(&module.path).join("static").join("src");
            Some((module.dir_name.to_string(), src.sanitize()))
        })
        .collect()
}

/// Collapse `.` and `..` lexically. The targets all exist under the workspace, so there is no
/// symlink subtlety worth a filesystem round trip here.
fn normalize(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve a module specifier as written in `importer` to an absolute JS file path: the
/// `@{module}/…` alias or a relative path (bare specifiers are npm packages → `None`).
/// Candidate order mirrors tsserver's; both forms normalize, since Odoo climbs out of
/// `static/src` through the alias (`@web/../lib/…`) and tsserver resolves that.
fn resolve_specifier(
    specifier: &str,
    importer: &str,
    module_src: &HashMap<String, String>,
    known: &HashSet<String>,
) -> Option<String> {
    let base = if let Some(rest) = specifier.strip_prefix('@') {
        let (module, sub_path) = rest.split_once('/')?;
        PathBuf::from(module_src.get(module)?).join(sub_path)
    } else if specifier.starts_with('.') {
        Path::new(importer).parent()?.join(specifier)
    } else {
        return None;
    };
    let base = normalize(base).sanitize();
    [
        base.clone(),
        format!("{base}.js"),
        format!("{base}.ts"),
        format!("{base}/index.js"),
    ]
    .into_iter()
    .find(|candidate| known.contains(candidate))
}

/// File → files declaring a direct subclass of one of its classes. Same-file subclasses are
/// skipped: they add no root.
fn subclass_file_edges(session: &SessionInfo) -> HashMap<String, HashSet<String>> {
    let descriptors = &session.sync_odoo.component_descriptors;
    let mut edges: HashMap<String, HashSet<String>> = HashMap::default();
    for descriptor in descriptors.values() {
        let Some(super_name) = descriptor.super_class_name.as_ref() else { continue };
        let Some(super_descriptor) = descriptors.get(super_name) else { continue };
        if super_descriptor.file_path != descriptor.file_path {
            edges
                .entry(super_descriptor.file_path.clone())
                .or_default()
                .insert(descriptor.file_path.clone());
        }
    }
    edges
}

/// The files that can hand a value declared in `origin` to *their* importers: `origin` itself, then
/// transitively anything re-exporting from an exposer or declaring a subclass of one of its
/// classes. Cycle-safe (each file enters the set once).
fn exposer_closure(
    reexporters: &HashMap<String, HashSet<String>>,
    subclasses: &HashMap<String, HashSet<String>>,
    origin: &str,
) -> HashSet<String> {
    let mut exposers: HashSet<String> = HashSet::default();
    exposers.insert(origin.to_string());
    let mut frontier = vec![origin.to_string()];
    while let Some(file) = frontier.pop() {
        let propagators = reexporters
            .get(&file)
            .into_iter()
            .flatten()
            .chain(subclasses.get(&file).into_iter().flatten());
        for next in propagators {
            if exposers.insert(next.clone()) {
                frontier.push(next.clone());
            }
        }
    }
    exposers
}

/// The exposers plus everything importing one of them, sorted for determinism.
fn roots_from(exposers: &HashSet<String>, importers: &HashMap<String, HashSet<String>>) -> Vec<String> {
    let mut roots: HashSet<String> = exposers.clone();
    for exposer in exposers {
        if let Some(direct) = importers.get(exposer) {
            roots.extend(direct.iter().cloned());
        }
    }
    let mut roots: Vec<String> = roots.into_iter().collect();
    roots.sort();
    roots
}

/// Every JS file that must be in tsserver's program for a find-references query against a
/// symbol declared in any of `origins` (its declaring file(s) unioned with the cursor's
/// file) to be complete. The graph is built once and the exposer closures of all origins
/// are unioned before their direct importers are added.
pub fn reference_roots(session: &SessionInfo, origins: &[String]) -> Vec<String> {
    let graph = ImportGraph::build(session);
    let subclasses = subclass_file_edges(session);
    let mut exposers: HashSet<String> = HashSet::default();
    for origin in origins {
        exposers.extend(exposer_closure(&graph.reexporters, &subclasses, origin));
    }
    roots_from(&exposers, &graph.importers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn map(entries: &[(&str, &[&str])]) -> HashMap<String, HashSet<String>> {
        entries.iter().map(|(k, v)| (k.to_string(), set(v))).collect()
    }

    #[test]
    fn normalize_collapses_dot_and_parent_components() {
        assert_eq!(normalize(PathBuf::from("/a/b/./c")), PathBuf::from("/a/b/c"));
        assert_eq!(normalize(PathBuf::from("/a/b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalize(PathBuf::from("/a/b/../../c/d")), PathBuf::from("/c/d"));
    }

    #[test]
    fn resolve_specifier_handles_aliases_relatives_and_bare_packages() {
        let module_src = [("web".to_string(), "/odoo/addons/web/static/src".to_string())]
            .into_iter()
            .collect();
        let known = set(&[
            "/odoo/addons/web/static/src/core/utils/hooks.js",
            "/odoo/addons/web/static/src/views/index.js",
            "/odoo/addons/web/static/src/views/list/list_renderer.js",
            "/odoo/addons/web/static/src/views/helpers.ts",
            "/odoo/addons/web/static/lib/hoot-dom/helpers/events.js",
        ]);
        let importer = "/odoo/addons/web/static/src/views/list/list_renderer.js";

        // `@module/*` alias, with the `.js` extension implied.
        assert_eq!(
            resolve_specifier("@web/core/utils/hooks", importer, &module_src, &known).as_deref(),
            Some("/odoo/addons/web/static/src/core/utils/hooks.js")
        );
        // A directory resolves through its `index.js`.
        assert_eq!(
            resolve_specifier("@web/views", importer, &module_src, &known).as_deref(),
            Some("/odoo/addons/web/static/src/views/index.js")
        );
        // Relative specifiers normalize against the importer's directory, and `.ts` is a candidate.
        assert_eq!(
            resolve_specifier("../helpers", importer, &module_src, &known).as_deref(),
            Some("/odoo/addons/web/static/src/views/helpers.ts")
        );
        // Odoo climbs out of `static/src` through the alias; tsserver resolves it, so must we.
        assert_eq!(
            resolve_specifier("@web/../lib/hoot-dom/helpers/events", importer, &module_src, &known)
                .as_deref(),
            Some("/odoo/addons/web/static/lib/hoot-dom/helpers/events.js")
        );
        // Bare packages, unknown modules and unresolvable paths are simply not edges.
        assert_eq!(resolve_specifier("@odoo/owl", importer, &module_src, &known), None);
        assert_eq!(resolve_specifier("luxon", importer, &module_src, &known), None);
        assert_eq!(resolve_specifier("@sale/thing", importer, &module_src, &known), None);
        assert_eq!(resolve_specifier("./missing", importer, &module_src, &known), None);
    }

    #[test]
    fn exposer_closure_follows_reexports_and_subclasses_only() {
        // barrel.js re-exports a.js; b.js subclasses a.js; c.js subclasses b.js.
        // leaf.js merely imports a.js and must NOT become an exposer.
        let reexporters = map(&[("a.js", &["barrel.js"])]);
        let subclasses = map(&[("a.js", &["b.js"]), ("b.js", &["c.js"])]);
        let exposers = exposer_closure(&reexporters, &subclasses, "a.js");
        assert_eq!(exposers, set(&["a.js", "barrel.js", "b.js", "c.js"]));

        // Querying from the middle of the chain does not walk back up to the base.
        assert_eq!(exposer_closure(&reexporters, &subclasses, "b.js"), set(&["b.js", "c.js"]));

        // A cycle terminates.
        let cyclic = map(&[("x.js", &["y.js"]), ("y.js", &["x.js"])]);
        assert_eq!(
            exposer_closure(&HashMap::default(), &cyclic, "x.js"),
            set(&["x.js", "y.js"])
        );
    }

    #[test]
    fn roots_from_adds_direct_importers_of_every_exposer_and_sorts() {
        let importers = map(&[
            ("a.js", &["leaf1.js", "b.js"]),
            ("b.js", &["leaf2.js"]),
            ("unrelated.js", &["leaf3.js"]),
        ]);
        let roots = roots_from(&set(&["a.js", "b.js"]), &importers);
        // Exposers themselves, plus the direct importers of each. `leaf3.js` imports a file that is
        // not an exposer, so it cannot hold the symbol and stays out.
        assert_eq!(roots, vec!["a.js", "b.js", "leaf1.js", "leaf2.js"]);
    }
}
