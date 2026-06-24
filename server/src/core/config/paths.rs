//! Path/template resolution for the config pipeline: `${...}` expansion and the
//! absolute-canonical form (`resolve_path`) every path setting is normalized to.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::S;
use crate::utils::{HashMap, PathSanitizer};

/// `${var}` template syntax.
static TEMPLATE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{([^}]+)\}").unwrap());
/// Captures the name in `${workspaceFolder:<name>}`.
static WS_FOLDER_NAMED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{workspaceFolder:([^}]+)\}").unwrap());
static HOME_DIR: LazyLock<Option<String>> =
    LazyLock::new(|| dirs::home_dir().map(|buf| buf.sanitize()));

/// Whether `resolve_path` follows a symlinked final path component. Defaults to
/// `Resolve`, so a field that declares no policy fully canonicalizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SymlinkPolicy {
    /// Resolve every symlink (`canonicalize`); two routes to one dir compare equal.
    #[default]
    Resolve,
    /// Canonicalize the parent but keep the leaf verbatim, so a venv's
    /// `bin/python` symlink survives — resolving it would hide the venv from
    /// Python's `pyvenv.cfg` detection.
    PreserveLeaf,
}

/// Make `path` absolute (relative paths resolve against `base`) and canonical
/// per `policy`. The path must exist, so an `Err` doubles as "not a path".
pub(super) fn resolve_path(
    path: impl AsRef<Path>,
    base: impl AsRef<Path>,
    policy: SymlinkPolicy,
) -> std::io::Result<String> {
    let path = path.as_ref();
    let absolute = if path.is_relative() {
        base.as_ref().join(path)
    } else {
        path.to_path_buf()
    };
    let resolved = match policy {
        SymlinkPolicy::Resolve => std::fs::canonicalize(&absolute)?,
        // No parent/leaf (root, or a `.`/`..` leaf) → nothing to preserve.
        SymlinkPolicy::PreserveLeaf => match (absolute.parent(), absolute.file_name()) {
            (Some(parent), Some(leaf)) => std::fs::canonicalize(parent)?.join(leaf),
            _ => std::fs::canonicalize(&absolute)?,
        },
    };
    Ok(resolved.sanitize())
}

/// Expand `${...}` variables, returning the filled string verbatim (still
/// possibly relative — pass to `resolve_path` to absolutize/validate). Supports
/// `${userHome}`, `${workspaceFolder}`, `${workspaceFolder:<name>}`, and `var_map`
/// (e.g. `${version}`/`${base}`). A missing/ambiguous named folder is an error.
pub(super) fn fill_template_path(
    unique_ws_folders: &HashMap<String, String>,
    current_ws: Option<&(String, String)>,
    template: &str,
    var_map: HashMap<String, String>,
) -> Result<String, String> {
    let mut pattern_map: HashMap<String, String> =
        build_pattern_map(unique_ws_folders).into_iter().chain(var_map).collect();
    if let Some((_, path)) = current_ws {
        pattern_map.insert(S!("workspaceFolder"), path.clone());
    }
    if template.contains("workspaceFolder:") {
        for cap in WS_FOLDER_NAMED_REGEX.captures_iter(template) {
            let ws_name = &cap[1];
            if !unique_ws_folders.contains_key(ws_name) {
                return Err(format!(
                    "Pattern '${{workspaceFolder:{ws_name}}}' ignored due to ambiguous or missing workspace name."
                ));
            }
        }
    }
    fill_template(template, &pattern_map)
}

pub(super) fn has_template(template: &str) -> bool {
    TEMPLATE_REGEX.is_match(template)
}

/// Substitute every `${key}` from `vars`; an unknown key is an error.
fn fill_template(template: &str, vars: &HashMap<String, String>) -> Result<String, String> {
    let mut invalid = None;
    let result = TEMPLATE_REGEX.replace_all(template, |captures: &regex::Captures| -> String {
        let key = captures[1].to_string();
        match vars.get(&key) {
            Some(value) => value.clone(),
            None => {
                invalid = Some(format!("Invalid key ({key}) in pattern"));
                S!("")
            }
        }
    });
    match invalid {
        Some(err) => Err(err),
        None => Ok(S!(result)),
    }
}

/// The always-available vars: `${userHome}` and `${workspaceFolder:<name>}`.
fn build_pattern_map(unique_ws_folders: &HashMap<String, String>) -> HashMap<String, String> {
    let mut pattern_map = HashMap::default();
    if let Some(home_dir) = HOME_DIR.as_ref() {
        pattern_map.insert(S!("userHome"), home_dir.clone());
    }
    for (ws_name, ws_path) in unique_ws_folders.iter() {
        pattern_map.insert(format!("workspaceFolder:{ws_name}"), ws_path.clone());
    }
    pattern_map
}

#[cfg(test)]
mod tests {
    use super::{SymlinkPolicy, resolve_path};
    use crate::utils::PathSanitizer;
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    use std::path::PathBuf;

    /// A `TempDir` whose root is canonical, so expected paths can be built from
    /// it without `/tmp`-vs-`/private/tmp` symlink surprises.
    fn canon_temp() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        (tmp, root)
    }

    #[test]
    fn absolutizes_relative_against_base() {
        let (tmp, root) = canon_temp();
        tmp.child("a/b").create_dir_all().unwrap();
        let got = resolve_path("a/b", tmp.path(), SymlinkPolicy::Resolve).unwrap();
        assert_eq!(got, root.join("a/b").sanitize());
    }

    #[test]
    fn collapses_dot_and_dotdot() {
        let (tmp, root) = canon_temp();
        tmp.child("a/b").create_dir_all().unwrap();
        let got = resolve_path("a/../a/b/.", tmp.path(), SymlinkPolicy::Resolve).unwrap();
        assert_eq!(got, root.join("a/b").sanitize());
    }

    #[test]
    fn missing_path_is_err() {
        let tmp = TempDir::new().unwrap();
        assert!(resolve_path("does/not/exist", tmp.path(), SymlinkPolicy::Resolve).is_err());
    }

    /// The venv case: `PreserveLeaf` keeps a symlinked leaf, `Resolve` follows it.
    #[cfg(unix)]
    #[test]
    fn leaf_symlink_followed_only_by_resolve() {
        use std::os::unix::fs::symlink;
        let (tmp, root) = canon_temp();
        tmp.child("usr/bin").create_dir_all().unwrap();
        tmp.child("usr/bin/python3.11").touch().unwrap();
        tmp.child("venv/bin").create_dir_all().unwrap();
        let link = tmp.child("venv/bin/python");
        symlink("../../usr/bin/python3.11", link.path()).unwrap();

        let preserved = resolve_path(link.path(), tmp.path(), SymlinkPolicy::PreserveLeaf).unwrap();
        assert_eq!(preserved, root.join("venv/bin/python").sanitize());

        let resolved = resolve_path(link.path(), tmp.path(), SymlinkPolicy::Resolve).unwrap();
        assert_eq!(resolved, root.join("usr/bin/python3.11").sanitize());
    }

    /// `PreserveLeaf` still resolves a symlinked *parent* while keeping the leaf.
    #[cfg(unix)]
    #[test]
    fn preserve_leaf_resolves_parent_symlinks() {
        use std::os::unix::fs::symlink;
        let (tmp, root) = canon_temp();
        tmp.child("real/bin").create_dir_all().unwrap();
        tmp.child("real/bin/python").touch().unwrap();
        symlink(tmp.child("real").path(), tmp.child("link").path()).unwrap();

        let got = resolve_path(tmp.child("link/bin/python").path(), tmp.path(), SymlinkPolicy::PreserveLeaf).unwrap();
        assert_eq!(got, root.join("real/bin/python").sanitize());
    }
}
