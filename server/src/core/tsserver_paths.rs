//! Generate `paths` for tsserver's "openExternalProject" command
//! 
//! These map imports in both ways:
//! - An import from "@web/foo" resolves to "addons/web/static/src/foo.js" (or .ts)
//! - when providing import completions to a source in
//!   "addons/web/static/src/foo.js", it gets inserted as @web/foo

use std::path::{Path, PathBuf};
use crate::core::js_utils::read_module_header;
use crate::utils::{HashMap, HashSet, PathSanitizer};

/// Generate the mapping of paths and glob-patterns to feed the tsserver. e.g.
/// ```ignore
///   "@web/*" -> ["/path/to/addons/web/static/src/*"]
///   "@web/../tests/*" -> ["/path/to/addons/web/static/tests/*"]
///   "@odoo/owl" -> ["/path/to/addons/web/static/src/@types/owl.d.ts"]
/// ```
pub fn generate_paths_map(addon_dirs: &[PathBuf])-> HashMap<String, Vec<String>> {
    let mut paths: HashMap<String, Vec<String>> = HashMap::default();
    for (name, root) in collect_modules(addon_dirs) {
        add_type_declaration_paths(&name, &root, &mut paths);
        add_glob_paths(&name, &root, &mut paths);
        add_lib_paths(&name, &root, &mut paths);
    }
    paths
}

// This has precedence over `add_lib_paths`, so `@odoo/hoot` keeps
// the typed `.d.ts` ahead of `hoot.js` when resolving an import .
fn add_type_declaration_paths(module_name: &str, root: &Path, paths: &mut HashMap<String, Vec<String>>) {
    if module_name != "web" { return }
    for (key, relative) in [
            ("@odoo/owl",  "static/src/@types/owl.d.ts"),
            ("@odoo/hoot", "static/src/@types/hoot.d.ts"),
    ] {
        let file_path = root.join(relative);
        if file_path.is_file() {
            paths.entry(key.to_string()).or_default().push(file_path.sanitize());
        }
    }
}

/// @foo/* -> addons/foo/static/src/*
/// @foo/../tests/* -> addons/foo/static/tests/*
fn add_glob_paths(module_name: &str, root: &Path, paths: &mut HashMap<String, Vec<String>>) {
    for (key, relative) in [
        (format!("@{module_name}/*"),           "static/src"),
        (format!("@{module_name}/../tests/*"),  "static/tests"),
    ] {
        let dir_path = root.join(relative);
        if dir_path.is_dir() {
            paths.entry(key).or_default().push(dir_path.join("*").sanitize());
        }
    }
}

/// `static/lib` gets no glob: only files with "@odoo-module" header are modules
fn add_lib_paths(module_name: &str, root: &Path, paths: &mut HashMap<String, Vec<String>>) {
    let lib_root = root.join("static/lib");
    for file in collect_js_files(&lib_root) {
        // Skip if no "@odoo-module" header
        let Some(header) = read_module_header(&file) else { continue };
        // Skip if "@odoo-module ignore"
        if header.ignore { continue };
        let abs_path = file.sanitize_cow();
        if let Some(relative_path) = lib_module_path(&lib_root, &file) {
            let key = format!("@{module_name}/../lib/{relative_path}");
            paths.entry(key).or_default().push(abs_path.to_string());
        }
        // alternative name provided by "@odoo-module alias=..."
        if let Some(alias) = header.alias {
            paths.entry(alias).or_default().push(abs_path.to_string());
        }
    }
}

/// Every Odoo module directory under `addon_dirs`, as `(name, path)`
fn collect_modules(addon_dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut modules = vec![];
    let mut visited = HashSet::default();
    // iterate over addon dirs
    for addon_dir in addon_dirs {
        // dedup addon_dir
        if !visited.insert(addon_dir) { continue };
        let Ok(entries) = std::fs::read_dir(addon_dir) else { continue };
        // iterate over module dirs
        for module_dir in entries.flatten() {
            let is_dir = module_dir.file_type().is_ok_and(|typ| typ.is_dir());
            if !is_dir { continue }
            let name = module_dir.file_name().to_string_lossy().to_string();
            let path = module_dir.path();
            modules.push((name, path))
        }
    }
    modules
}

/// Recursively search js/ts files under dir
fn collect_js_files(dir: &Path) -> Vec<PathBuf> {
    fn collect_js_files_inner(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|typ| typ.is_dir()) {
                if entry.file_name().to_str() != Some("node_modules") {
                    collect_js_files_inner(&path, out);
                }
            } else if matches!(path.extension().and_then(|ext| ext.to_str()), Some("js" | "ts")) {
                out.push(path);
            }
        }
    }
    let mut out = vec![];
    collect_js_files_inner(dir, &mut out);
    out
}

/// `/.../addons/module/static/lib/a/b.js` -> `a/b`.
/// Extension and trailing "/index" dropped, per `url_to_module_path` (`odoo/tools/js_transpiler.py`)
fn lib_module_path(lib_root: &Path, file: &Path) -> Option<String> {
    // remove extension
    let mut path = file.with_extension("");
    // remove trailing index
    if path.file_name().is_some_and(|name| name == "index") {
        path.pop();
    }
    // remove prefix
    Some(path.strip_prefix(lib_root).ok()?.sanitize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths_mirror_url_to_module_path() {
        let root = Path::new("/odoo/addons/web/static/lib");
        let path = |file: &str| lib_module_path(root, &root.join(file));
    
        // extension gets removed
        assert_eq!(path("hoot-dom/helpers/time.js").as_deref(), Some("hoot-dom/helpers/time"));
        assert_eq!(path("hoot-dom/hoot-dom.ts").as_deref(), Some("hoot-dom/hoot-dom"));
        // A directory's `index.js` is imported as the directory.
        assert_eq!(path("hoot/index.js").as_deref(), Some("hoot"));
    }

}
