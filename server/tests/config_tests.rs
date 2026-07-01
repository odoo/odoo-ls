//! Integration tests for the `odools.toml` configuration pipeline.
//!
//! Organized by feature area (see the `===` section banners). All tests share
//! the `Cfg` fixture and the `make_*` / `write_odools` helpers below, which build
//! a temp workspace tree and resolve the configuration.

use assert_fs::TempDir;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use lsp_types::Uri;
use odoo_ls_server::S;
use odoo_ls_server::core::config::render::ProfileView;
use odoo_ls_server::core::config::{
    ConfigEntry, ConfigKey, ConfigView, config_json_schema, get_configuration, needs_restart,
};
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::{HashMap, HashSet, PathSanitizer};
use std::fs;
use std::path::Path;
use std::str::FromStr;

/// A workspace/config fixture: builds a temp dir tree, registers workspace
/// folders, and resolves the configuration.
struct Cfg {
    temp: TempDir,
    workspaces: Vec<(String, String)>,
    config_path: Option<String>,
}

impl Cfg {
    fn new() -> Self {
        Cfg {
            temp: TempDir::new().unwrap(),
            workspaces: Vec::new(),
            config_path: None,
        }
    }
    /// Create and register a workspace folder named `name`; returns its handle.
    fn ws(&mut self, name: &str) -> ChildPath {
        let dir = self.temp.child(name);
        dir.create_dir_all().unwrap();
        self.workspaces.push((S!(name), canonicalized(dir.path())));
        dir
    }
    /// Register a workspace folder under `name` backed by a distinct directory
    /// `dir_name`. Use when two folders must share a name but differ in path.
    fn ws_named(&mut self, name: &str, dir_name: &str) -> ChildPath {
        let dir = self.temp.child(dir_name);
        dir.create_dir_all().unwrap();
        self.workspaces.push((S!(name), canonicalized(dir.path())));
        dir
    }
    /// A directory under the temp root that is NOT a registered workspace folder.
    fn dir(&self, name: &str) -> ChildPath {
        let d = self.temp.child(name);
        d.create_dir_all().unwrap();
        d
    }
    /// Resolve against a standalone config file (the session `config_path`).
    fn with_config_file(&mut self, path: String) -> &mut Self {
        self.config_path = Some(path);
        self
    }
    /// Resolve the configuration.
    fn resolve(&self) -> Result<(HashMap<String, ConfigEntry>, ConfigView), String> {
        let mut session = mock_session(&self.workspaces);
        session.sync_odoo.config_path = self.config_path.clone();
        get_configuration(&mut session)
    }
    /// Resolve, expecting success: `(runtime map, render view)`.
    fn ok(&self) -> (HashMap<String, ConfigEntry>, ConfigView) {
        self.resolve().expect("config resolution should succeed")
    }
    /// The resolved runtime entry for `name` (panics if absent).
    fn entry(&self, name: &str) -> ConfigEntry {
        self.ok().0.remove(name).unwrap_or_else(|| panic!("no profile '{name}'"))
    }
    /// The "default" runtime entry.
    fn default(&self) -> ConfigEntry {
        self.entry("default")
    }
    /// The render view.
    fn view(&self) -> ConfigView {
        self.ok().1
    }
    /// The render view of profile `name` (panics if absent).
    fn profile(&self, name: &str) -> ProfileView {
        self.view()
            .entries()
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("no profile view '{name}'"))
    }
    /// Resolve, expecting an error; returns the message.
    fn err(&self) -> String {
        self.resolve().expect_err("config resolution should fail")
    }
}

/// Mock session with the given `(name, sanitized path)` workspace folders.
fn mock_session(workspaces: &[(String, String)]) -> SessionInfo<'static> {
    let sync_odoo = Box::leak(Box::new(SyncOdoo::new()));
    {
        let file_mgr = sync_odoo.get_file_mgr();
        let mut file_mgr = file_mgr.borrow_mut();
        for (name, path) in workspaces {
            file_mgr.add_workspace_folder(name.clone(), Uri::from_str(path).unwrap());
        }
    }
    SessionInfo::new(sync_odoo)
}

fn canonicalized(path: &Path) -> String {
    fs::canonicalize(path).unwrap().sanitize()
}

/// Write `contents` to `<dir>/odools.toml`.
fn write_odools<P: PathChild>(dir: &P, contents: &str) {
    dir.child("odools.toml").write_str(contents).unwrap();
}

/// Make `dir` a valid Odoo checkout (creates `odoo/release.py`).
fn make_odoo<P: PathChild>(dir: &P) {
    dir.child("odoo").create_dir_all().unwrap();
    dir.child("odoo").child("release.py").touch().unwrap();
}

/// Create module dir `name` (with `__manifest__.py`) under `parent`; returns it.
fn make_addon<P: PathChild>(parent: &P, name: &str) -> ChildPath {
    let m = parent.child(name);
    m.create_dir_all().unwrap();
    m.child("__manifest__.py").touch().unwrap();
    m
}
// ===========================================================================
// Auto-detection
// ===========================================================================

#[test]
fn single_workspace_root_is_addon_path() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");
    make_addon(&ws, "my_module");

    let config = cfg.default();
    assert!(config.addons_paths().contains(&canonicalized(ws.path())));
}

#[test]
fn multiple_workspaces_only_addon_roots_detected() {
    let mut cfg = Cfg::new();
    let ws1 = cfg.ws("ws1");
    let ws2 = cfg.ws("ws2");
    // ws1 has an addon subdir; ws2 has none.
    make_addon(&ws1, "addon1");

    let config = cfg.default();
    assert!(config.addons_paths().contains(&canonicalized(ws1.path())));
    assert!(!config.addons_paths().contains(&canonicalized(ws2.path())));
}

#[test]
fn workspace_root_is_odoo_path() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("odoo_ws");
    make_odoo(&ws);

    let config = cfg.default();
    assert_eq!(config.odoo_path().as_ref(), Some(&canonicalized(ws.path())));
}

#[test]
fn odoo_path_detected_from_workspace_child() {
    let mut cfg = Cfg::new();
    let src = cfg.ws("src");
    // src/odoo is a valid odoo checkout (src/odoo/odoo/release.py).
    let odoo = src.child("odoo");
    odoo.create_dir_all().unwrap();
    make_odoo(&odoo);

    let config = cfg.default();
    assert_eq!(config.odoo_path(), Some(canonicalized(odoo.path())));
}

#[test]
fn addons_paths_detected_from_workspace_children() {
    let mut cfg = Cfg::new();
    let src = cfg.ws("src");
    let enterprise = src.child("enterprise");
    enterprise.create_dir_all().unwrap();
    make_addon(&enterprise, "account");
    let themes = src.child("design-themes");
    themes.create_dir_all().unwrap();
    make_addon(&themes, "theme_default");

    let addons = cfg.default().addons_paths();
    assert!(addons.contains(&canonicalized(enterprise.path())),
        "expected {}, got {addons:?}", canonicalized(enterprise.path()));
    assert!(addons.contains(&canonicalized(themes.path())),
        "expected {}, got {addons:?}", canonicalized(themes.path()));
}

/// `src/` containing `odoo/`, addon folders auto-detects the odoo path and all
/// addon children (covers wiki "open a src/ folder" scenario).
#[test]
fn full_src_folder_scenario() {
    let mut cfg = Cfg::new();
    let src = cfg.ws("src");

    let odoo = src.child("odoo");
    odoo.create_dir_all().unwrap();
    make_odoo(&odoo);

    let enterprise = src.child("enterprise");
    enterprise.create_dir_all().unwrap();
    make_addon(&enterprise, "account");

    let custom = src.child("custom-addons");
    custom.create_dir_all().unwrap();
    make_addon(&custom, "my_mod");

    let themes = src.child("design-themes");
    themes.create_dir_all().unwrap();
    make_addon(&themes, "theme_default");

    let config = cfg.default();
    assert_eq!(config.odoo_path(), Some(canonicalized(odoo.path())));
    assert!(config.addons_paths().contains(&canonicalized(enterprise.path())));
    assert!(config.addons_paths().contains(&canonicalized(custom.path())));
    assert!(config.addons_paths().contains(&canonicalized(themes.path())));
}

/// A workspace folder that is directly a valid odoo_path is used as-is;
/// child-directory detection only applies when the root does not match.
#[test]
fn direct_workspace_match_used_for_odoo_path() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("odoo_root");
    make_odoo(&ws);

    assert_eq!(cfg.default().odoo_path().as_ref(), Some(&canonicalized(ws.path())));
}

/// User-configured paths always win over auto-detection: a child that is a
/// valid odoo_path is ignored when odoo_path is set explicitly.
#[test]
fn user_config_overrides_auto_detection() {
    let mut cfg = Cfg::new();
    let src = cfg.ws("src");

    // Auto-detectable child odoo path.
    let auto_odoo = src.child("odoo");
    auto_odoo.create_dir_all().unwrap();
    make_odoo(&auto_odoo);

    // User config points to a different odoo path.
    let user_odoo = cfg.dir("user_odoo");
    make_odoo(&user_odoo);

    write_odools(&src, &format!(
        "[[config]]\nname = \"default\"\nodoo_path = \"{}\"\n",
        canonicalized(user_odoo.path())
    ));

    assert_eq!(cfg.default().odoo_path().as_ref(), Some(&canonicalized(user_odoo.path())));
}

#[test]
fn conflict_two_children_both_odoo_path() {
    let mut cfg = Cfg::new();
    let src = cfg.ws("src");

    let odoo1 = src.child("odoo1");
    odoo1.create_dir_all().unwrap();
    make_odoo(&odoo1);
    let odoo2 = src.child("odoo2");
    odoo2.create_dir_all().unwrap();
    make_odoo(&odoo2);

    assert!(cfg.err().contains("More than one workspace folder or subfolder is a valid odoo_path"));
}

#[test]
fn child_addons_detected_when_root_is_also_addon() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    // Root is itself an addon path.
    make_addon(&ws, "mod1");
    // A child subdir is also an addon path.
    let subdir = ws.child("subdir");
    subdir.create_dir_all().unwrap();
    make_addon(&subdir, "mod2");

    let addons = cfg.default().addons_paths();
    assert!(addons.contains(&canonicalized(ws.path())));
    assert!(addons.contains(&canonicalized(subdir.path())));
}

#[test]
fn addons_detected_nested_below_immediate_children() {
    // The addons dir sits two levels below the workspace folder
    // (ws/group/myaddons/mod) — unreachable by an immediate-children-only scan.
    let mut cfg = Cfg::new();
    let ws = cfg.ws("src");
    let myaddons = ws.child("group").child("myaddons");
    let m = myaddons.child("mod");
    m.create_dir_all().unwrap();
    m.child("__manifest__.py").touch().unwrap();

    let addons = cfg.default().addons_paths();
    assert!(addons.contains(&canonicalized(myaddons.path())),
        "expected nested addons dir to be detected, got {addons:?}");
    assert!(!addons.contains(&canonicalized(ws.child("group").path())),
        "intermediate dir should not be an addons path, got {addons:?}");
}

#[test]
fn recursive_scan_prunes_modules_and_skipped_dirs() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("src");

    // A real addons dir at depth 1.
    let real = ws.child("realaddons");
    let real_mod = real.child("mod");
    real_mod.create_dir_all().unwrap();
    real_mod.child("__manifest__.py").touch().unwrap();
    // An addons-looking dir nested inside a module — must not be detected.
    let buried = real_mod.child("sub_addons");
    let submod = buried.child("submod");
    submod.create_dir_all().unwrap();
    submod.child("__manifest__.py").touch().unwrap();

    // Addons inside hidden / build dirs — must be skipped outright.
    for skip in [".hidden", "node_modules", "__pycache__"] {
        let s = ws.child(skip).child("skipaddons").child("smod");
        s.create_dir_all().unwrap();
        s.child("__manifest__.py").touch().unwrap();
    }

    let addons = cfg.default().addons_paths();
    assert!(addons.contains(&canonicalized(real.path())),
        "real addons dir should be detected, got {addons:?}");
    assert!(!addons.contains(&canonicalized(buried.path())),
        "addons buried inside a module must be pruned, got {addons:?}");
    for skip in [".hidden", "node_modules", "__pycache__"] {
        let s = canonicalized(ws.child(skip).child("skipaddons").path());
        assert!(!addons.contains(&s), "addons under {skip} must be skipped, got {addons:?}");
    }
}

#[test]
fn odoo_path_requires_release_py() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    let fake_odoo = cfg.dir("fake_odoo");
    fake_odoo.child("odoo").create_dir_all().unwrap();

    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\nodoo_path = \"{}\"\n",
        canonicalized(fake_odoo.path())
    ));

    assert!(cfg.default().odoo_path().is_none(),
        "directory without odoo/release.py should not be a valid odoo_path");
}

#[test]
fn addon_path_requires_manifest() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    let not_addon = cfg.dir("not_addon");
    not_addon.child("some_dir").create_dir_all().unwrap();

    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\n",
        canonicalized(not_addon.path())
    ));

    assert!(!cfg.default().addons_paths().contains(&canonicalized(not_addon.path())),
        "directory without any __manifest__.py should not be a valid addon path");
}
// ===========================================================================
// Basic config, shadowing & extends
// ===========================================================================

#[test]
fn single_odools_toml_scalars() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        python_path = 'python'
        file_cache = false
        auto_refresh_delay = 1234
    "#);

    let cfg = c.default();
    assert_eq!(cfg.python_path(), "python");
    assert!(!cfg.file_cache());
    assert_eq!(cfg.auto_refresh_delay(), 1234);

    // Render view serializes the same values.
    let html = c.view().to_html_string();
    assert!(html.contains("python"));
    assert!(html.contains("false"));
    assert!(html.contains("1234"));
}

// ===========================================================================
// Path canonicalization & symlinks
// ===========================================================================

/// The absolute path of a real Python interpreter (its `sys.executable`), or
/// `None` if none is available — symlink tests below need a genuinely runnable
/// target because `is_python_path` executes the binary.
#[cfg(unix)]
fn real_python() -> Option<std::path::PathBuf> {
    for cmd in ["python3", "python"] {
        if let Ok(out) = std::process::Command::new(cmd)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            && out.status.success()
        {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(std::path::PathBuf::from(p));
            }
        }
    }
    None
}

/// A `python_path` pointing at a virtualenv interpreter (a symlink to the base
/// interpreter) keeps the venv path — resolving the symlink would hide the venv
/// from Python's `pyvenv.cfg` detection. The template form skips
/// `canon_python_path`'s "already runnable" early return, so this exercises the
/// `PreserveLeaf` canonicalization.
#[cfg(unix)]
#[test]
fn python_path_preserves_venv_symlink() {
    let Some(base_interpreter) = real_python() else {
        eprintln!("skipping python_path_preserves_venv_symlink: no python interpreter found");
        return;
    };
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    let venv_bin = ws.child("venv").child("bin");
    venv_bin.create_dir_all().unwrap();
    let python = venv_bin.child("python");
    std::os::unix::fs::symlink(&base_interpreter, python.path()).unwrap();

    write_odools(&ws, r#"
        [[config]]
        name = "default"
        python_path = "${workspaceFolder}/venv/bin/python"
    "#);

    let cfg = c.default();
    // The venv path is kept verbatim (only the parent is canonicalized)...
    assert_eq!(cfg.python_path(), python.path().sanitize());
    // ...and is NOT collapsed to the base interpreter the symlink points at.
    assert_ne!(cfg.python_path(), base_interpreter.sanitize());
}

/// In contrast, a directory setting (`odoo_path`) fully resolves symlinks, so
/// two routes to the same checkout compare equal.
#[cfg(unix)]
#[test]
fn odoo_path_resolves_symlink() {
    let mut c = Cfg::new();
    let real_odoo = c.dir("real_odoo");
    make_odoo(&real_odoo);
    let ws = c.ws("ws1");
    std::os::unix::fs::symlink(real_odoo.path(), ws.child("odoo_link").path()).unwrap();

    write_odools(&ws, r#"
        [[config]]
        name = "default"
        odoo_path = "${workspaceFolder}/odoo_link"
    "#);

    let cfg = c.default();
    // The symlink is followed down to the real checkout.
    assert_eq!(cfg.odoo_path().as_deref(), Some(real_odoo.path().sanitize().as_str()));
    assert_ne!(cfg.odoo_path().as_deref(), Some(ws.child("odoo_link").path().sanitize().as_str()));
}

#[test]
fn minimal_config_named_profile_with_odoo_path() {
    // Wiki minimal config: a named profile pointing at an Odoo checkout.
    let mut c = Cfg::new();
    let community = c.dir("community");
    make_odoo(&community);
    let ws = c.ws("my_project");
    write_odools(&ws, &format!(r#"[[config]]
name = "My Config"
odoo_path = "{}"
"#, canonicalized(community.path())));

    let cfg = c.entry("My Config");
    assert_eq!(cfg.odoo_path().as_ref().unwrap(), &canonicalized(community.path()));
}

#[test]
fn workspace_folder_token_in_addons_paths() {
    // ${workspaceFolder} expands to the current workspace folder.
    let mut c = Cfg::new();
    let ws = c.ws("my_addons");
    make_addon(&ws, "my_module");
    write_odools(&ws, r#"[[config]]
name = "default"
addons_paths = ["${workspaceFolder}"]
"#);

    assert!(c.default().addons_paths().contains(&canonicalized(ws.path())));
}

#[test]
fn name_defaults_to_default_when_omitted() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    write_odools(&ws, r#"[[config]]
file_cache = false
"#);

    let cfg = c.default();
    assert!(!cfg.file_cache());
}

#[test]
fn deeper_odools_toml_shadows_shallower() {
    // Parent (shallower) provides auto_refresh_delay; the workspace (deeper)
    // file shadows python_path and file_cache.
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&c.temp, r#"
        [[config]]
        name = "default"
        python_path = "python3"
        file_cache = true
        auto_refresh_delay = 1111
    "#);
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        python_path = "python"
        file_cache = false
    "#);

    let cfg = c.default();
    // Deeper file wins for set keys; an unset key falls back to the parent.
    assert_eq!(cfg.python_path(), "python");
    assert!(!cfg.file_cache());
    assert_eq!(cfg.auto_refresh_delay(), 1111);

    let html = c.view().to_html_string();
    assert!(html.contains("python"));
    assert!(html.contains("false"));
    assert!(html.contains("1111"));
}

#[test]
fn dir_walk_list_fields_merge_across_files() {
    // Scalar keys override deepest-wins, but list fields (addons_paths) merge
    // across the parent/child dir walk.
    let mut c = Cfg::new();
    let ws = c.ws("project");
    let parent_addons = c.dir("shared_addons");
    make_addon(&parent_addons, "shared_mod");
    let child_addons = ws.child("local_addons");
    child_addons.create_dir_all().unwrap();
    make_addon(&child_addons, "local_mod");

    write_odools(&c.temp, &format!(r#"[[config]]
name = "default"
addons_paths = ["{}"]
"#, canonicalized(parent_addons.path())));
    write_odools(&ws, &format!(r#"[[config]]
name = "default"
addons_paths = ["{}"]
"#, canonicalized(child_addons.path())));

    let paths = c.default().addons_paths();
    assert!(paths.contains(&canonicalized(parent_addons.path())));
    assert!(paths.contains(&canonicalized(child_addons.path())));
}

#[test]
fn extends_inherits_and_child_shadows() {
    // `default` extends `base`, overriding python_path; the workspace file
    // overrides auto_refresh_delay; file_cache is inherited from base.
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&c.temp, r#"
        [[config]]
        name = "base"
        python_path = "python3"
        file_cache = true
        auto_refresh_delay = 2222

        [[config]]
        name = "default"
        extends = "base"
        python_path = "python"
    "#);
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 3333
    "#);

    let cfg = c.default();
    assert_eq!(cfg.python_path(), "python");
    assert!(cfg.file_cache());
    assert_eq!(cfg.auto_refresh_delay(), 3333);

    let html = c.view().to_html_string();
    assert!(html.contains("python"));
    assert!(html.contains("true"));
    assert!(html.contains("3333"));
}

#[test]
fn extends_basic_inherits_scalar_and_path() {
    // Wiki: child extends a sibling profile, inheriting scalars + odoo_path
    // while adding its own addons_paths.
    let mut c = Cfg::new();
    let community = c.dir("community");
    make_odoo(&community);
    let ws = c.ws("my_addons");
    let project_a = ws.child("project_a");
    project_a.create_dir_all().unwrap();
    make_addon(&project_a, "mod_a");

    write_odools(&ws, &format!(r#"[[config]]
name = "base_setup"
file_cache = false
odoo_path = "{}"

[[config]]
name = "project A"
extends = "base_setup"
addons_paths = ["./project_a"]
"#, canonicalized(community.path())));

    let cfg = c.entry("project A");
    assert!(!cfg.file_cache());
    assert_eq!(cfg.odoo_path().as_ref().unwrap(), &canonicalized(community.path()));
    assert!(cfg.addons_paths().contains(&canonicalized(project_a.path())));
}

#[test]
fn extends_can_target_profile_from_parent_file() {
    // extends resolves profiles defined in shallower odools.toml files up the tree.
    let mut c = Cfg::new();
    let community = c.dir("community");
    make_odoo(&community);
    let ws = c.ws("project");
    let addons = ws.child("my_addons");
    addons.create_dir_all().unwrap();
    make_addon(&addons, "mod_a");

    write_odools(&c.temp, &format!(r#"[[config]]
name = "base_setup"
file_cache = false
odoo_path = "{}"
"#, canonicalized(community.path())));
    write_odools(&ws, r#"[[config]]
name = "my_project"
extends = "base_setup"
addons_paths = ["./my_addons"]
"#);

    let cfg = c.entry("my_project");
    assert!(!cfg.file_cache());
    assert_eq!(cfg.odoo_path().as_ref().unwrap(), &canonicalized(community.path()));
    assert!(cfg.addons_paths().contains(&canonicalized(addons.path())));
}

#[test]
fn extends_chain_resolves_regardless_of_order_and_file() {
    // base <- mid <- default chain: each scalar is inherited from the right link,
    // stable across declaration order and the file a profile lives in.
    let mut c = Cfg::new();
    let ws = c.ws("ws1");

    // mid declared before base; default extends mid.
    write_odools(&c.temp, r#"
        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "base"
        auto_refresh_delay = 1111
        ac_filter_model_names = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#);
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        ac_filter_model_names = true
    "#);

    let cfg = c.default();
    assert!(!cfg.file_cache());
    assert_eq!(cfg.auto_refresh_delay(), 1111);
    assert_eq!(format!("{:?}", cfg.diag_missing_imports()).to_lowercase(), "onlyodoo");
    assert!(cfg.ac_filter_model_names());

    // Swap base/mid order in the parent file: result unchanged.
    write_odools(&c.temp, r#"
        [[config]]
        name = "base"
        auto_refresh_delay = 1111
        ac_filter_model_names = false

        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#);
    let cfg2 = c.default();
    assert!(!cfg2.file_cache());
    assert_eq!(cfg2.auto_refresh_delay(), 1111);
    assert_eq!(format!("{:?}", cfg2.diag_missing_imports()).to_lowercase(), "onlyodoo");
    assert!(cfg2.ac_filter_model_names());

    // Move `base` into the workspace (deeper) file, mid+default in parent: the
    // chain still resolves, and now ac_filter_model_names comes from base.
    write_odools(&ws, r#"
        [[config]]
        name = "base"
        auto_refresh_delay = 1111
        ac_filter_model_names = false
    "#);
    write_odools(&c.temp, r#"
        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#);
    let cfg3 = c.default();
    assert!(!cfg3.file_cache());
    assert_eq!(cfg3.auto_refresh_delay(), 1111);
    assert_eq!(format!("{:?}", cfg3.diag_missing_imports()).to_lowercase(), "onlyodoo");
    assert!(!cfg3.ac_filter_model_names());
}

#[test]
fn extends_cycle_in_chain_errors() {
    // base <-> mid cycle reached via default's extends chain.
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&c.temp, r#"
        [[config]]
        name = "base"
        extends = "mid"
        auto_refresh_delay = 111

        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#);
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        ac_filter_model_names = true
    "#);

    assert!(c.err().contains("Circular dependency detected"));
}

#[test]
fn extends_rootless_cycle_errors() {
    // Two profiles extending each other, neither reachable from a "default"
    // root — must still be detected as a cycle.
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&ws, r#"
        [[config]]
        name = "a"
        extends = "b"

        [[config]]
        name = "b"
        extends = "a"
    "#);

    assert!(c.err().contains("Circular dependency detected"));
}

#[test]
fn extends_nonexistent_profile_errors() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        extends = "doesnotexist"
        file_cache = false
    "#);

    assert!(c.err().to_lowercase().contains("extends non-existing profile"));
}
// ===========================================================================
// Template variables, relative paths & provenance
// ===========================================================================

#[test]
fn template_workspacefolder_variations() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    make_addon(&ws1, "my_module");
    let ws2 = c.ws("ws2");
    make_addon(&ws2, "my_module2");

    write_odools(
        &ws1,
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${workspaceFolder}",
            "${workspaceFolder:ws1}",
            "${workspaceFolder:ws2}",
            "${workspaceFolder:doesnotexist}",
        ]
    "#,
    );

    let cfg = c.default();
    assert!(cfg.addons_paths().iter().any(|p| p == &canonicalized(ws1.path())));
    assert!(cfg.addons_paths().iter().any(|p| p == &canonicalized(ws2.path())));
    assert!(!cfg.addons_paths().iter().any(|p| p.ends_with("doesnotexist")));
}

#[test]
fn template_workspacefolder_cross_reference_and_auto_detect() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    make_addon(&ws1, "addon1");
    let ws2 = c.ws("ws2");
    make_addon(&ws2, "addon2");

    // ws1's config only references ws2 via template: ws1 should NOT be added.
    write_odools(
        &ws1,
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${workspaceFolder:ws2}"
        ]
    "#,
    );
    let cfg = c.default();
    assert!(!cfg.addons_paths().iter().any(|p| p == &canonicalized(ws1.path())));
    assert!(cfg.addons_paths().iter().any(|p| p == &canonicalized(ws2.path())));

    // With $autoDetectAddons, ws1 should be added as well.
    write_odools(
        &ws1,
        r#"
        [[config]]
        name = "default"
        addons_paths = ["$autoDetectAddons"]
    "#,
    );
    let cfg = c.default();
    assert!(cfg.addons_paths().iter().any(|p| p == &canonicalized(ws1.path())));
    assert!(cfg.addons_paths().iter().any(|p| p == &canonicalized(ws2.path())));
}

#[test]
fn template_userhome_and_workspacefolder_expansion() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");

    // Dummy addon under the user home.
    let user_home = dirs::home_dir().unwrap();
    let user_home_addon_path = user_home.join("my_home_addons").join("my_home_addon");
    std::fs::create_dir_all(&user_home_addon_path).unwrap();
    std::fs::File::create(user_home_addon_path.join("__manifest__.py")).unwrap();

    make_addon(&ws1, "my_ws1_addon");

    write_odools(
        &ws1,
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${userHome}/my_home_addons",
            "${workspaceFolder:ws1}"
        ]
    "#,
    );

    let cfg = c.default();
    let expected_home_addon = canonicalized(&user_home.join("my_home_addons"));
    assert!(cfg.addons_paths().contains(&expected_home_addon));
    assert!(cfg.addons_paths().contains(&canonicalized(ws1.path())));
}

#[test]
fn relative_addons_paths_in_workspace_config() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");

    let mod1 = ws.child("addons1").child("mod1");
    mod1.create_dir_all().unwrap();
    mod1.child("__manifest__.py").touch().unwrap();

    let mod2 = ws.child("addons2").child("mod2");
    mod2.create_dir_all().unwrap();
    mod2.child("__manifest__.py").touch().unwrap();

    write_odools(
        &ws,
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "./addons1",
            "./addons2"
        ]
    "#,
    );

    let cfg = c.default();
    let expected1 = canonicalized(ws.child("addons1").path());
    let expected2 = canonicalized(ws.child("addons2").path());
    assert!(cfg.addons_paths().contains(&expected1), "Expected addons_paths to contain {}", expected1);
    assert!(cfg.addons_paths().contains(&expected2), "Expected addons_paths to contain {}", expected2);
}

#[test]
fn relative_addons_paths_resolved_from_parent_config_dir() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");

    // addons live next to the parent (temp-root) odools.toml, not the workspace.
    let addons1 = c.temp.child("addons1");
    let mod1 = addons1.child("mod1");
    mod1.create_dir_all().unwrap();
    mod1.child("__manifest__.py").touch().unwrap();

    let addons2 = c.temp.child("addons2");
    let mod2 = addons2.child("mod2");
    mod2.create_dir_all().unwrap();
    mod2.child("__manifest__.py").touch().unwrap();

    // Parent config at temp root; workspace has no odools.toml.
    write_odools(
        &c.temp,
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "./addons1",
            "./addons2"
        ]
    "#,
    );

    let cfg = c.default();
    let expected1 = canonicalized(addons1.path());
    let expected2 = canonicalized(addons2.path());
    // Resolved relative to the config dir (temp root), independent of `ws`.
    let _ = ws;
    assert!(cfg.addons_paths().contains(&expected1), "Expected addons_paths to contain {}", expected1);
    assert!(cfg.addons_paths().contains(&expected2), "Expected addons_paths to contain {}", expected2);
}

#[test]
fn path_case_and_trailing_slash_normalization() {
    let mut c = Cfg::new();
    let ws = c.ws("Workspace1");

    let addon_dir = ws.child("My_Module").child("");
    addon_dir.create_dir_all().unwrap();
    addon_dir.child("__manifest__.py").touch().unwrap();

    let with_slash = canonicalized(ws.child("").path());
    let without_slash = canonicalized(ws.path());

    let mut normalized_addon = canonicalized(ws.path());
    if normalized_addon.ends_with('/') {
        normalized_addon.pop();
    }

    // Both forms together collapse to a single normalized path.
    write_odools(
        &ws,
        &format!(
            r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}",
            "{}"
        ]
    "#,
            with_slash, without_slash,
        ),
    );
    assert!(c.default().addons_paths().iter().any(|p| p == &normalized_addon));

    // Only the trailing-slash form.
    write_odools(
        &ws,
        &format!(
            r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
    "#,
            with_slash
        ),
    );
    assert!(c.default().addons_paths().iter().any(|p| p == &normalized_addon));

    // Only the non-slash form.
    write_odools(
        &ws,
        &format!(
            r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
    "#,
            without_slash
        ),
    );
    assert!(c.default().addons_paths().iter().any(|p| p == &normalized_addon));
}

#[test]
fn provenance_single_file_sources() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    let f = ws.child("odools.toml");
    f.write_str(
        r#"
        [[config]]
        name = "default"
        python_path = "python"
        file_cache = false
        auto_refresh_delay = 1234
    "#,
    )
    .unwrap();

    let cfg = c.profile("default");
    let src = canonicalized(f.path());

    assert_eq!(cfg.scalar_sources(ConfigKey::PythonPath).unwrap().len(), 1);
    assert!(cfg.scalar_sources(ConfigKey::PythonPath).unwrap().contains(&src));
    assert_eq!(cfg.scalar_sources(ConfigKey::FileCache).unwrap().len(), 1);
    assert!(cfg.scalar_sources(ConfigKey::FileCache).unwrap().contains(&src));
    assert_eq!(cfg.scalar_sources(ConfigKey::AutoRefreshDelay).unwrap().len(), 1);
    assert!(cfg.scalar_sources(ConfigKey::AutoRefreshDelay).unwrap().contains(&src));
}

#[test]
fn provenance_multiple_files_and_extends() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");

    let parent = c.temp.child("odools.toml");
    parent
        .write_str(
            r#"
        [[config]]
        name = "base"
        python_path = "python3"
        file_cache = true
        auto_refresh_delay = 2222

        [[config]]
        name = "default"
        extends = "base"
        python_path = "python"
    "#,
        )
        .unwrap();

    let ws_f = ws.child("odools.toml");
    ws_f.write_str(
        r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 3333
    "#,
    )
    .unwrap();

    let cfg = c.profile("default");
    // python_path comes from the parent's root config.
    assert!(cfg.scalar_sources(ConfigKey::PythonPath).unwrap().contains(&canonicalized(parent.path())));
    // file_cache comes from the parent's base config (via extends).
    assert!(cfg.scalar_sources(ConfigKey::FileCache).unwrap().contains(&canonicalized(parent.path())));
    // auto_refresh_delay comes from the workspace file (overrides parent).
    assert!(cfg.scalar_sources(ConfigKey::AutoRefreshDelay).unwrap().contains(&canonicalized(ws_f.path())));
}

#[test]
fn provenance_template_variable_workspacefolder() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    make_addon(&ws1, "addon1");
    let ws2 = c.ws("ws2");
    make_addon(&ws2, "addon2");

    let f = ws1.child("odools.toml");
    f.write_str(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${workspaceFolder}",
            "${workspaceFolder:ws2}"
        ]
    "#,
    )
    .unwrap();

    let cfg = c.profile("default");
    let src = canonicalized(f.path());
    let addons = cfg.list(ConfigKey::AddonsPaths);
    // Both ws1 and ws2 are present, each sourced from ws1's odools.toml.
    assert!(addons.iter().any(|s| s.value() == &canonicalized(ws1.path()) && s.sources().contains(&src)));
    assert!(addons.iter().any(|s| s.value() == &canonicalized(ws2.path()) && s.sources().contains(&src)));
}

#[test]
fn provenance_multiple_workspace_folders_merge_sources() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    let ws2 = c.ws("ws2");

    let f1 = ws1.child("odools.toml");
    f1.write_str(
        r#"
        [[config]]
        name = "default"
        python_path = "python"
    "#,
    )
    .unwrap();
    let f2 = ws2.child("odools.toml");
    f2.write_str(
        r#"
        [[config]]
        name = "default"
        python_path = "python"
    "#,
    )
    .unwrap();

    let cfg = c.profile("default");
    // Single merged "default" entry, value "python" from ws1 (merged in order).
    assert_eq!(cfg.str_value(ConfigKey::PythonPath).unwrap(), "python");
    // Sources include both workspace files.
    let sources = cfg.scalar_sources(ConfigKey::PythonPath).unwrap();
    assert!(sources.contains(&canonicalized(f1.path())));
    assert!(sources.contains(&canonicalized(f2.path())));
}

#[test]
fn provenance_default_values_marked_as_default() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    // No odools.toml: every value is a default, sourced from "$default".
    let _ = ws;

    let cfg = c.profile("default");
    assert!(cfg
        .scalar_sources(ConfigKey::PythonPath)
        .unwrap()
        .iter()
        .any(|s| s == "$default"));
}

#[test]
fn provenance_json_serialization() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    make_addon(&ws1, "addon1");
    write_odools(
        &ws1,
        r#"
        [[config]]
        name = "default"
    "#,
    );

    let json = serde_json::to_value(c.view()).unwrap();
    let config_arr = json.get("config").unwrap().as_array().unwrap();
    let root = config_arr.iter().find(|p| p.get("name").unwrap() == "default").unwrap();

    // python_path is a scalar {"value","sources","info"} sourced from $default.
    let python_path = root.get("python_path").unwrap();
    assert!(python_path
        .get("sources")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "$default"));

    // addons_paths is a list of those objects; auto-detected ws1 is sourced
    // from the workspaceFolder:ws1 token.
    let addons_paths = root.get("addons_paths").unwrap().as_array().unwrap();
    assert!(addons_paths.iter().any(|ap| {
        ap.get("sources")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("workspaceFolder:ws1"))
    }));
}
// ===========================================================================
// Cross-workspace merge, extra config file & list merge/override
// ===========================================================================

/// Two workspace folders that are both valid odoo paths produce an ambiguity error.
#[test]
fn conflict_two_workspace_folders_both_odoo_path() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    let ws2 = c.ws("ws2");
    make_odoo(&ws1);
    make_odoo(&ws2);

    assert!(c.err().contains("More than one workspace folder or subfolder is a valid odoo_path"));
}

/// Two workspaces setting different values for a scalar (file_cache) conflict.
#[test]
fn conflict_two_workspaces_scalar_field() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    let ws2 = c.ws("ws2");
    write_odools(&ws1, "[[config]]\nname = \"default\"\nfile_cache = true\n");
    write_odools(&ws2, "[[config]]\nname = \"default\"\nfile_cache = false\n");

    assert!(c.err().contains("Conflict detected"));
}

/// No conflict when both workspace configs explicitly set odoo_path to the same path.
#[test]
fn no_conflict_when_config_files_point_to_same_odoo_path() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    let ws2 = c.ws("ws2");
    make_odoo(&ws1); // only ws1 is a valid odoo path
    let toml = format!(
        "[[config]]\nname = \"default\"\nodoo_path = \"{}\"\n",
        canonicalized(ws1.path())
    );
    write_odools(&ws1, &toml);
    write_odools(&ws2, &toml);

    assert_eq!(c.default().odoo_path().as_ref().unwrap(), &canonicalized(ws1.path()));
}

/// Same odoo_path across two workspaces, but different addons_paths: all addons merge,
/// and the shared addons path is sourced from both odools.toml files.
#[test]
fn merge_different_odoo_paths_and_addons_paths() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    let ws2 = c.ws("ws2");
    make_odoo(&ws1);

    let shared = c.dir("shared_addons");
    make_addon(&shared, "shared_mod");
    let ws1_addons = ws1.child("addons1");
    ws1_addons.create_dir_all().unwrap();
    make_addon(&ws1_addons, "mod1");
    let ws2_addons = ws2.child("addons2");
    ws2_addons.create_dir_all().unwrap();
    make_addon(&ws2_addons, "mod2");

    write_odools(&ws1, &format!(
        "[[config]]\nname = \"default\"\nodoo_path = \"{}\"\naddons_paths = [\"{}\", \"{}\"]\n",
        canonicalized(ws1.path()), canonicalized(shared.path()), canonicalized(ws1_addons.path())
    ));
    write_odools(&ws2, &format!(
        "[[config]]\nname = \"default\"\nodoo_path = \"{}\"\naddons_paths = [\"{}\", \"{}\"]\n",
        canonicalized(ws1.path()), canonicalized(shared.path()), canonicalized(ws2_addons.path())
    ));

    let expected = [
        canonicalized(shared.path()),
        canonicalized(ws1_addons.path()),
        canonicalized(ws2_addons.path()),
    ].into_iter().collect::<HashSet<_>>();
    assert_eq!(c.default().addons_paths().clone(), expected);

    // Shared addons path is sourced from both ws1 and ws2 odools.toml.
    let view = c.view();
    let entry = view.entries().iter().find(|p| p.name == "default").unwrap();
    let sources: Vec<_> = entry
        .list(ConfigKey::AddonsPaths)
        .iter()
        .filter(|s| s.value() == &canonicalized(shared.path()))
        .flat_map(|s| s.sources())
        .cloned()
        .collect();
    assert!(sources.iter().any(|src| src.ends_with("ws1/odools.toml")));
    assert!(sources.iter().any(|src| src.ends_with("ws2/odools.toml")));
}

/// Boolean field set in the parent (shallower) toml is inherited; set in the child
/// it overrides.
#[test]
fn merge_boolean_field_parent_inherited_child_overrides() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&c.temp, "[[config]]\nname = \"default\"\nfile_cache = false\nac_filter_model_names = false\n");
    write_odools(&ws, "[[config]]\nname = \"default\"\nac_filter_model_names = true\n");

    let cfg = c.default();
    assert!(!cfg.file_cache());
    assert!(cfg.ac_filter_model_names());
}

/// addons_merge: default merge keeps parent + child; "override" drops the parent list.
#[test]
fn addons_merge_default_merges_override_drops_parent() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    let parent_addons = c.dir("parent_addons");
    make_addon(&parent_addons, "parent_mod");
    let child_addons = ws.child("child_addons");
    child_addons.create_dir_all().unwrap();
    make_addon(&child_addons, "child_mod");

    write_odools(&c.temp, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\n",
        canonicalized(parent_addons.path())
    ));

    // Default (merge): both present.
    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\naddons_merge = \"merge\"\n",
        canonicalized(child_addons.path())
    ));
    let paths = c.default().addons_paths().clone();
    assert!(paths.contains(&canonicalized(parent_addons.path())));
    assert!(paths.contains(&canonicalized(child_addons.path())));

    // Override: only the child path.
    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\naddons_merge = \"override\"\n",
        canonicalized(child_addons.path())
    ));
    let paths = c.default().addons_paths().clone();
    assert_eq!(paths, [canonicalized(child_addons.path())].into_iter().collect::<HashSet<_>>());
}

/// addons_merge = "override" with no addons_paths set: workspace is auto-detected if
/// valid; with an empty list it yields no addons.
#[test]
fn addons_merge_override_unset_vs_empty() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    let parent_addons = c.dir("parent_addons");
    make_addon(&parent_addons, "parent_mod");
    make_addon(&ws, "mod3");

    write_odools(&c.temp, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\n",
        canonicalized(parent_addons.path())
    ));

    // override + no addons_paths: workspace auto-detected, parent dropped.
    write_odools(&ws, "[[config]]\nname = \"default\"\naddons_merge = \"override\"\n");
    let paths = c.default().addons_paths().clone();
    assert!(paths.contains(&canonicalized(ws.path())));
    assert!(!paths.contains(&canonicalized(parent_addons.path())));

    // override + addons_paths = []: empty.
    write_odools(&ws, "[[config]]\nname = \"default\"\naddons_paths = []\naddons_merge = \"override\"\n");
    assert!(c.default().addons_paths().is_empty());
}

/// addons_merge = "override" with $autoDetectAddons: parent dropped, auto-detection kept.
#[test]
fn addons_merge_override_with_auto_detect_token() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    let parent_addons = c.dir("parent_addons");
    make_addon(&parent_addons, "parent_mod");
    make_addon(&ws, "ws_mod");

    write_odools(&c.temp, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\n",
        canonicalized(parent_addons.path())
    ));
    write_odools(&ws, "[[config]]\nname = \"default\"\naddons_paths = [\"$autoDetectAddons\"]\naddons_merge = \"override\"\n");

    let paths = c.default().addons_paths().clone();
    assert!(paths.contains(&canonicalized(ws.path())), "auto-detected workspace kept");
    assert!(!paths.contains(&canonicalized(parent_addons.path())), "parent dropped by override");
}

/// addons_merge declared only in the parent (shallower) toml is ignored: the control
/// key is read from the child only, so the lists merge.
#[test]
fn addons_merge_control_key_in_parent_is_ignored() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    let parent_addons = c.dir("parent_addons");
    make_addon(&parent_addons, "mod1");
    let ws_addons = ws.child("ws_addons");
    ws_addons.create_dir_all().unwrap();
    make_addon(&ws_addons, "mod2");

    write_odools(&c.temp, &format!(
        "[[config]]\nname = \"default\"\naddons_merge = \"override\"\naddons_paths = [\"{}\"]\n",
        canonicalized(parent_addons.path())
    ));
    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\"]\n",
        canonicalized(ws_addons.path())
    ));

    let expected = [canonicalized(parent_addons.path()), canonicalized(ws_addons.path())]
        .into_iter().collect::<HashSet<_>>();
    assert_eq!(c.default().addons_paths().clone(), expected,
        "addons_merge set only in the parent must be ignored, so the lists merge");
}

/// addons_paths unset auto-detects the workspace; addons_paths = [] disables detection.
#[test]
fn addons_paths_unset_vs_empty() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    make_addon(&ws, "mod1");

    // Unset: workspace auto-detected.
    write_odools(&ws, "[[config]]\nname = \"default\"\n");
    assert!(c.default().addons_paths().contains(&canonicalized(ws.path())),
        "Workspace should be added when addons_paths is not set");

    // Empty: detection disabled.
    write_odools(&ws, "[[config]]\nname = \"default\"\naddons_paths = []\n");
    assert!(c.default().addons_paths().is_empty(),
        "addons_paths = [] should disable auto-detection");
}

/// $autoDetectAddons combines a manual path with auto-detected workspace addons.
#[test]
fn auto_detect_addons_token_combines_manual_and_detected() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    make_addon(&ws, "auto_module");
    let custom = c.dir("custom_addons");
    make_addon(&custom, "custom_module");

    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\naddons_paths = [\"{}\", \"$autoDetectAddons\"]\n",
        canonicalized(custom.path())
    ));

    let paths = c.default().addons_paths().clone();
    assert!(paths.contains(&canonicalized(custom.path())), "manual path present");
    assert!(paths.contains(&canonicalized(ws.path())), "auto-detected workspace present");
}

/// additional_stubs_merge: "override" drops the parent stub, default merge keeps both.
#[test]
fn additional_stubs_merge_override_vs_merge() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    let parent_stub = c.dir("parent_stub");
    let child_stub = ws.child("child_stub");
    child_stub.create_dir_all().unwrap();

    write_odools(&c.temp, &format!(
        "[[config]]\nname = \"default\"\nadditional_stubs = [\"{}\"]\n",
        canonicalized(parent_stub.path())
    ));

    // Override: only the child stub.
    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\nadditional_stubs = [\"{}\"]\nadditional_stubs_merge = \"override\"\n",
        canonicalized(child_stub.path())
    ));
    let stubs = c.default().additional_stubs().clone();
    assert!(stubs.contains(&canonicalized(child_stub.path())), "override: child stub present");
    assert!(!stubs.contains(&canonicalized(parent_stub.path())), "override: parent stub dropped");

    // Merge (default): both stubs.
    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\nadditional_stubs = [\"{}\"]\nadditional_stubs_merge = \"merge\"\n",
        canonicalized(child_stub.path())
    ));
    let stubs = c.default().additional_stubs().clone();
    assert!(stubs.contains(&canonicalized(child_stub.path())), "merge: child stub present");
    assert!(stubs.contains(&canonicalized(parent_stub.path())), "merge: parent stub present");
}

/// An invalid merge-method value is rejected at parse time with a clear error.
#[test]
fn invalid_merge_method_value_errors() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    write_odools(&ws, "[[config]]\nname = \"default\"\naddons_merge = \"concat\"\n");

    let err = c.err();
    assert!(err.contains("invalid value 'concat'") && err.contains("addons_merge"),
        "unexpected error: {err}");
}

/// The standalone extra config file (config_path) is merged with the workspace config:
/// its scalars apply and its addons_paths merge with the workspace's.
#[test]
fn extra_config_file_merged_with_workspace() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    let ws_addon = ws.child("ws_addons");
    ws_addon.create_dir_all().unwrap();
    make_addon(&ws_addon, "ws_mod");
    let ext_addon = c.dir("ext_addons");
    make_addon(&ext_addon, "ext_mod");

    write_odools(&ws, &format!(
        "[[config]]\nname = \"default\"\npython_path = \"python\"\naddons_paths = [\"{}\"]\n",
        canonicalized(ws_addon.path())
    ));

    let ext = c.dir("cfg").child("external_config.toml");
    ext.write_str(&format!(
        "[[config]]\nname = \"default\"\nfile_cache = false\nauto_refresh_delay = 4321\naddons_paths = [\"{}\"]\n",
        canonicalized(ext_addon.path())
    )).unwrap();
    c.with_config_file(canonicalized(ext.path()));

    let cfg = c.default();
    assert!(!cfg.file_cache());
    assert_eq!(cfg.auto_refresh_delay(), 4321);
    let paths = cfg.addons_paths().clone();
    assert!(paths.contains(&canonicalized(ws_addon.path())), "should contain ws_addon path");
    assert!(paths.contains(&canonicalized(ext_addon.path())), "should contain ext_addon path");
}

/// A non-existent extra config file path errors.
#[test]
fn extra_config_file_nonexistent_errors() {
    let mut c = Cfg::new();
    c.ws("ws1");
    let missing = c.dir("cfg").child("does_not_exist.toml");
    c.with_config_file(missing.path().sanitize());

    assert!(!c.err().is_empty());
}
// ===========================================================================
// Version resolution & splitting
// ===========================================================================

/// `$version` + `${version}` template: odoo_path resolves to `<base>/<version>/odoo`.
#[test]
fn version_variable_in_odoo_path() {
    let mut c = Cfg::new();
    // <root>/18.0/odoo/odoo/release.py and <root>/17.0/odoo/odoo/release.py
    for v in ["18.0", "17.0"] {
        let odoo = c.dir(&format!("{v}/odoo/odoo"));
        odoo.child("release.py").touch().unwrap();
    }

    // Static $version resolved through odoo_path template.
    let ws = c.ws("ws");
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        "$version" = "18.0"
        odoo_path = "../${version}/odoo"
    "#);
    let cfg = c.entry("default");
    assert_eq!(cfg.odoo_path().as_ref().unwrap(), &canonicalized(c.dir("18.0/odoo").path()));
}

/// `$version = "${workspaceFolder}/.."` derives the version from the parent
/// directory name; odoo_path uses a template rooted at that version dir.
#[test]
fn version_from_workspace_parent_dir() {
    for (version, ws_name) in [("18.0", "ws18"), ("17.0", "ws17")] {
        let mut c = Cfg::new();
        let odoo = c.dir(&format!("{version}/odoo/odoo"));
        odoo.child("release.py").touch().unwrap();

        let ws = c.ws(&format!("{version}/addons"));
        make_addon(&ws, "addon1");
        write_odools(&ws, r#"
            [[config]]
            "$version" = "${workspaceFolder}/.."
            odoo_path = "${workspaceFolder}/../odoo"
        "#);

        let cfg = c.entry("default");
        assert_eq!(
            cfg.odoo_path().as_ref().unwrap(),
            &canonicalized(c.dir(&format!("{version}/odoo")).path()),
            "odoo_path should resolve to {version}/odoo for workspace {ws_name}"
        );
    }
}

/// `$version` pointed at a `__manifest__.py` extracts the version string from it,
/// then odoo_path templating resolves to that version's odoo source.
#[test]
fn version_from_manifest_file() {
    for version in ["18.0", "17.0"] {
        let mut c = Cfg::new();
        // <root>/<version>/odoo/odoo/release.py
        let odoo = c.dir(&format!("{version}/odoo/odoo"));
        odoo.child("release.py").touch().unwrap();

        let ws = c.ws(&format!("{version}/addons"));
        let addon = ws.child("my_addon");
        addon.create_dir_all().unwrap();
        addon
            .child("__manifest__.py")
            .write_str(&format!("{{ 'version': '{version}.1.0.0' }}"))
            .unwrap();

        write_odools(&ws, r#"
            [[config]]
            name = "default"
            "$version" = "${workspaceFolder}/my_addon/__manifest__.py"
            odoo_path = "${workspaceFolder}/../odoo"
        "#);

        let cfg = c.entry("default");
        assert_eq!(
            cfg.odoo_path().as_ref().unwrap(),
            &canonicalized(c.dir(&format!("{version}/odoo")).path()),
            "manifest version {version}.1.0.0 should select {version}/odoo"
        );
    }
}

/// `${splitVersion}` scans for version directories and spawns one non-abstract
/// profile per version; the source profile is kept and marked abstract.
#[test]
fn split_version_spawns_profile_per_version() {
    let mut c = Cfg::new();
    let ws = c.ws("ws1");
    // ws1/17.0/addon17 and ws1/18.0/addon18
    let v17 = ws.child("17.0");
    make_addon(&v17, "addon17");
    let v18 = ws.child("18.0");
    make_addon(&v18, "addon18");

    write_odools(&ws, r#"
        [[config]]
        name = "root"
        "$version" = "${workspaceFolder}/${splitVersion}"
        addons_paths = [
            "./${version}"
        ]
    "#);

    let (map, _) = c.ok();
    assert!(map.contains_key("root"), "should contain abstract root profile");
    assert!(map.contains_key("root-17.0"), "should contain root-17.0 profile");
    assert!(map.contains_key("root-18.0"), "should contain root-18.0 profile");

    assert!(c.profile("root").is_abstract(), "source profile must be abstract");
    assert!(!c.profile("root-17.0").is_abstract(), "root-17.0 must not be abstract");
    assert!(!c.profile("root-18.0").is_abstract(), "root-18.0 must not be abstract");

    // Each spawned profile carries only its own version's addons path.
    assert!(c.entry("root-17.0").addons_paths().contains(&canonicalized(v17.path())));
    assert!(c.entry("root-18.0").addons_paths().contains(&canonicalized(v18.path())));
}

/// `${splitVersion}` recognizes SaaS-style version directory names (e.g. `saas~16.1`)
/// alongside standard `X.0` names.
#[test]
fn split_version_recognizes_saas_format() {
    let mut c = Cfg::new();
    let ws = c.ws("project");
    let saas = ws.child("saas~16.1");
    make_addon(&saas, "saas_mod");
    let v18 = ws.child("18.0");
    make_addon(&v18, "mod_18");

    write_odools(&ws, r#"
        [[config]]
        "$version" = "${workspaceFolder}/${splitVersion}"
        addons_paths = ["${workspaceFolder}/${version}"]
    "#);

    let (map, _) = c.ok();
    assert!(map.contains_key("default-saas~16.1"),
        "SaaS version not detected, got: {:?}", map.keys().collect::<Vec<_>>());
    assert!(map.contains_key("default-18.0"));
}

/// Spawned profile names follow the `{original_name}-{version}` convention,
/// preserving a custom profile name (not forcing "default").
#[test]
fn split_version_naming_convention() {
    let mut c = Cfg::new();
    let ws = c.ws("project");
    let v17 = ws.child("17.0");
    make_addon(&v17, "mod_17");

    write_odools(&ws, r#"
        [[config]]
        name = "My Config"
        "$version" = "${workspaceFolder}/${splitVersion}"
        addons_paths = ["${workspaceFolder}/${version}"]
    "#);

    let (map, _) = c.ok();
    assert!(map.contains_key("My Config-17.0"),
        "Expected 'My Config-17.0', got: {:?}", map.keys().collect::<Vec<_>>());
}

/// `$base` + `${detectVersion}`: detection walks up from the workspace folder to
/// the version dir, sets `${version}`/`${base}`, and resolves paths against it —
/// whether the workspace is the version dir, its odoo subdir, or an addon subdir.
#[test]
fn base_and_detect_version_resolve_for_subpaths() {
    let c = Cfg::new();
    let vdir = c.dir("17.0");
    let odoo_dir = vdir.child("odoo");
    odoo_dir.create_dir_all().unwrap();
    odoo_dir.child("odoo").child("release.py").touch().unwrap();
    let addon_dir = vdir.child("addon-path");
    addon_dir.child("mod1").create_dir_all().unwrap();
    addon_dir.child("mod1").child("__manifest__.py").touch().unwrap();

    let base = canonicalized(c.temp.path());

    // ${base}-style references.
    let toml_base = format!(r#"
        [[config]]
        name = "default"
        "$base" = "{base}/${{detectVersion}}"
        odoo_path = "${{base}}/odoo"
        addons_paths = [ "${{base}}/addon-path" ]
    "#);
    // ${version}-style references (equivalent result).
    let toml_version = format!(r#"
        [[config]]
        name = "default"
        "$base" = "{base}/${{detectVersion}}"
        odoo_path = "{base}/${{version}}/odoo"
        addons_paths = [ "{base}/${{version}}/addon-path" ]
    "#);

    for toml in [&toml_base, &toml_version] {
        // Workspace placed at the version dir, the odoo subdir, and the addon subdir.
        for ws_path in [
            canonicalized(vdir.path()),
            canonicalized(odoo_dir.path()),
            canonicalized(addon_dir.path()),
        ] {
            let mut c2 = Cfg::new();
            // Recreate the tree in a fresh fixture so the workspace folder is registered.
            let v = c2.dir("17.0");
            let od = v.child("odoo");
            od.create_dir_all().unwrap();
            od.child("odoo").child("release.py").touch().unwrap();
            let ad = v.child("addon-path");
            ad.child("mod1").create_dir_all().unwrap();
            ad.child("mod1").child("__manifest__.py").touch().unwrap();
            let base2 = canonicalized(c2.temp.path());
            let toml2 = toml.replace(&base, &base2);
            c2.temp.child("odools.toml").write_str(&toml2).unwrap();
            // Register whichever subpath this iteration targets.
            let sub = ws_path.replace(&base, &base2);
            c2.workspaces.push((S!("ws"), sub));

            let cfg = c2.entry("default");
            assert_eq!(cfg.odoo_path().as_ref().unwrap(), &canonicalized(od.path()));
            assert!(cfg.addons_paths().contains(&canonicalized(ad.path())));
        }
    }

    // $base as an absolute (non-existent) path → error.
    let mut c_abs = Cfg::new();
    let v = c_abs.dir("17.0");
    v.create_dir_all().unwrap();
    c_abs.temp.child("odools.toml").write_str(r#"
        [[config]]
        name = "default"
        "$base" = "/not/a/real/path/${detectVersion}"
        odoo_path = "${base}/odoo"
        addons_paths = [ "${base}/addon-path" ]
    "#).unwrap();
    c_abs.workspaces.push((S!("ws_abs"), canonicalized(v.path())));
    assert!(c_abs.resolve().is_err(), "absolute $base should error");

    // $base that is not a valid path → error.
    let mut c_inv = Cfg::new();
    let v = c_inv.dir("17.0");
    v.create_dir_all().unwrap();
    c_inv.temp.child("odools.toml").write_str(r#"
        [[config]]
        name = "default"
        "$base" = "invalid_path/${detectVersion}"
        odoo_path = "${base}/odoo"
        addons_paths = [ "${base}/addon-path" ]
    "#).unwrap();
    c_inv.workspaces.push((S!("ws_invalid"), canonicalized(v.path())));
    assert!(c_inv.resolve().is_err(), "invalid $base should error");
}

/// When both `${detectVersion}` (via `$base`) and `${splitVersion}` (via `$version`)
/// are set, detection runs first and overwrites `$version`, preventing splitting.
#[test]
fn version_detection_overrides_splitting() {
    let mut c = Cfg::new();
    for v in ["17.0", "18.0"] {
        let vd = c.dir(v);
        vd.child("odoo").create_dir_all().unwrap();
        make_odoo(&vd);
    }
    let base = canonicalized(c.temp.path());
    let ws = c.ws("17.0/my_ws");

    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        "$base" = "{base}/${{detectVersion}}"
        "$version" = "{base}/${{splitVersion}}"
        odoo_path = "${{base}}/odoo"
    "#));

    let (map, _) = c.ok();
    assert!(
        !map.keys().any(|k| k.contains("-17.0") || k.contains("-18.0")),
        "detection should prevent splitting; profiles: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    assert!(map.contains_key("default"), "the single default profile survives");
}

/// `"$version"` must be a quoted TOML key — an unquoted `$version` is invalid TOML
/// and fails to parse.
#[test]
fn version_key_must_be_quoted() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    write_odools(&ws, r#"[[config]]
name = "default"
$version = "18.0"
"#);
    assert!(c.resolve().is_err(), "unquoted $version should fail TOML parsing");
}

/// `"$version"` must be a string, not a TOML float — `"$version" = 18.0` fails to
/// deserialize.
#[test]
fn version_must_be_string_not_float() {
    let mut c = Cfg::new();
    let ws = c.ws("ws");
    write_odools(&ws, r#"[[config]]
name = "default"
"$version" = 18.0
"#);
    assert!(c.resolve().is_err(), "$version as a float should fail deserialization");
}

/// Restart contract: a change in `$version` between two resolved configs must
/// trigger a restart; an identical `$version` must not.
#[test]
fn version_change_triggers_restart() {
    let mut a = ConfigEntry::new();
    let mut b = ConfigEntry::new();
    a.set_str(ConfigKey::Version, S!("17.0"));
    b.set_str(ConfigKey::Version, S!("18.0"));
    assert!(needs_restart(&a, &b), "a $version change must trigger a restart");

    let mut c = ConfigEntry::new();
    c.set_str(ConfigKey::Version, S!("17.0"));
    assert!(!needs_restart(&a, &c), "identical $version must not trigger a restart");
}
// ===========================================================================
// Diagnostic settings & filters
// ===========================================================================

#[test]
fn diagnostic_filter_example_codes_paths_path_type() {
    let mut cfg = Cfg::new();
    let odoo = cfg.dir("odoo");
    make_odoo(&odoo);
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        &format!(
            r#"[[config]]
name = "odoo"
odoo_path = "{}"

[[config.diagnostic_filters]]
codes = ["OLS.*"]
paths = ["**/my_folder_1/*", "**/account*/*"]
path_type = "notin"
"#,
            canonicalized(odoo.path())
        ),
    );

    let config = cfg.entry("odoo");
    assert_eq!(config.diagnostic_filters().len(), 1);
    let filter = &config.diagnostic_filters()[0];

    // codes should contain the OLS.* regex
    assert_eq!(filter.codes.len(), 1);
    assert!(filter.codes[0].is_match("OLS12345"));

    // paths should have 2 glob patterns
    assert_eq!(filter.paths.len(), 2);

    // path_type should be NotIn
    assert_eq!(format!("{:?}", filter.path_type), "NotIn");
}

#[test]
fn diagnostic_filter_path_type_defaults_to_in() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"

[[config.diagnostic_filters]]
paths = ["**/test/*"]
codes = ["OLS.*"]
"#,
    );

    let config = cfg.default();
    assert_eq!(config.diagnostic_filters().len(), 1);
    assert_eq!(format!("{:?}", config.diagnostic_filters()[0].path_type), "In");
}

#[test]
fn diagnostic_filter_types_all_severities() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"

[[config.diagnostic_filters]]
paths = ["**/*"]
types = ["Error", "Warning", "Info", "Hint"]
"#,
    );

    let config = cfg.default();
    assert_eq!(config.diagnostic_filters().len(), 1);
    assert_eq!(config.diagnostic_filters()[0].types.len(), 4);
}

#[test]
fn diagnostic_filter_paths_required() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"

[[config.diagnostic_filters]]
codes = ["OLS.*"]
"#,
    );

    // `paths` is required: the error must name the missing field, not just fail.
    assert!(cfg.err().contains("paths"), "error should mention the missing 'paths' field");
}

#[test]
fn diagnostic_filter_path_variable_expansion() {
    let user_home = dirs::home_dir().map(|buf| canonicalized(&buf)).unwrap();
    let version = "vX.Y.Z";

    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");
    let ws_path = canonicalized(ws.path());
    write_odools(
        &ws,
        &format!(
            r#"[[config]]
name = "default"
"$version" = "{version}"
[[config.diagnostic_filters]]
paths = ["${{userHome}}/some/path", "${{workspaceFolder}}/foo", "${{version}}/bar"]
"#
        ),
    );

    let config = cfg.default();
    let filters = config.diagnostic_filters();
    assert!(!filters.is_empty(), "Expected at least one diagnostic filter");
    let patterns: Vec<String> =
        filters[0].paths.iter().map(|p| p.as_str().to_string()).collect();
    // $userHome expanded
    assert!(
        patterns.iter().any(|p| p.starts_with(&format!("{user_home}/some/path"))),
        "userHome variable not expanded: {patterns:?}"
    );
    // ${workspaceFolder} expanded
    assert!(
        patterns.iter().any(|p| p.ends_with("/foo") && p.contains(&ws_path)),
        "workspaceFolder variable not expanded: {patterns:?}"
    );
    // $version expanded
    assert!(
        patterns.iter().any(|p| p.ends_with("/bar") && p.contains(version)),
        "version variable not expanded: {patterns:?}"
    );
}

#[test]
fn diagnostic_settings_table_syntax() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "odoo"

[config.diagnostic_settings]
OLS03001 = "Info"
"#,
    );

    let config = cfg.entry("odoo");
    let diagnostic_settings = config.diagnostic_settings();
    assert!(!diagnostic_settings.is_empty());
    let setting = diagnostic_settings.iter().find(|(code, _)| code.to_string() == "OLS03001");
    assert!(setting.is_some(), "OLS03001 should be in diagnostic_settings");
    assert_eq!(format!("{:?}", setting.unwrap().1), "Info");
}

#[test]
fn diagnostic_settings_inline_syntax() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"
diagnostic_settings = { "OLS03001" = "Disabled", "OLS02001" = "Warning" }
"#,
    );

    let config = cfg.default();
    let diagnostic_settings = config.diagnostic_settings();

    let ols03001 = diagnostic_settings.iter().find(|(code, _)| code.to_string() == "OLS03001");
    assert!(ols03001.is_some());
    assert_eq!(format!("{:?}", ols03001.unwrap().1), "Disabled");

    let ols02001 = diagnostic_settings.iter().find(|(code, _)| code.to_string() == "OLS02001");
    assert!(ols02001.is_some());
    assert_eq!(format!("{:?}", ols02001.unwrap().1), "Warning");
}

#[test]
fn diagnostic_settings_all_values() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"

[config.diagnostic_settings]
OLS03001 = "Error"
OLS02001 = "Warning"
OLS03002 = "Info"
OLS03003 = "Hint"
OLS03004 = "Disabled"
"#,
    );

    let config = cfg.default();
    let find_setting = |code: &str| -> String {
        config
            .diagnostic_settings()
            .iter()
            .find(|(c, _)| c.to_string() == code)
            .map(|(_, s)| format!("{s:?}"))
            .unwrap_or_default()
    };

    assert_eq!(find_setting("OLS03001"), "Error");
    assert_eq!(find_setting("OLS02001"), "Warning");
    assert_eq!(find_setting("OLS03002"), "Info");
    assert_eq!(find_setting("OLS03003"), "Hint");
    assert_eq!(find_setting("OLS03004"), "Disabled");
}

#[test]
fn diagnostic_filters_accumulated_on_extends() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "base"

[[config.diagnostic_filters]]
paths = ["**/base_path/*"]
codes = ["OLS01.*"]

[[config]]
name = "child"
extends = "base"

[[config.diagnostic_filters]]
paths = ["**/child_path/*"]
codes = ["OLS02.*"]
"#,
    );

    let config = cfg.entry("child");
    assert_eq!(
        config.diagnostic_filters().len(),
        2,
        "Expected 2 filters (parent + child), got {}",
        config.diagnostic_filters().len()
    );
}

#[test]
fn diagnostic_settings_override_on_extends() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "base"

[config.diagnostic_settings]
OLS03001 = "Error"
OLS02001 = "Warning"

[[config]]
name = "child"
extends = "base"

[config.diagnostic_settings]
OLS03001 = "Info"
"#,
    );

    let config = cfg.entry("child");
    let find_setting = |code: &str| -> String {
        config
            .diagnostic_settings()
            .iter()
            .find(|(c, _)| c.to_string() == code)
            .map(|(_, s)| format!("{s:?}"))
            .unwrap_or_default()
    };

    // OLS03001 overridden by child; OLS02001 inherited from parent.
    assert_eq!(find_setting("OLS03001"), "Info");
    assert_eq!(find_setting("OLS02001"), "Warning");
}

#[test]
fn diagnostic_filter_disabled_type_not_allowed() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"

[[config.diagnostic_filters]]
paths = ["**/*"]
types = ["Disabled"]
"#,
    );

    assert!(
        cfg.err().contains("Disabled"),
        "error should explain that 'Disabled' is not an allowed filter type"
    );
}

#[test]
fn diagnostic_filter_invalid_regex_error() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"

[[config.diagnostic_filters]]
paths = ["**/*"]
codes = ["[invalid regex"]
"#,
    );

    assert!(
        cfg.err().to_lowercase().contains("regex"),
        "error should point at the invalid regex"
    );
}

#[test]
fn diagnostic_keys_in_reference_table() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"
diagnostic_settings = { "OLS03001" = "Disabled" }

[[config.diagnostic_filters]]
paths = ["**/test/*"]
codes = ["OLS.*"]
"#,
    );

    let config = cfg.default();
    assert!(!config.diagnostic_settings().is_empty());
    assert!(!config.diagnostic_filters().is_empty());
}
// ===========================================================================
// Scalars, defaults, invalid config, rejection surfacing, restart & schema
// ===========================================================================

#[test]
fn invalid_toml_config_errors() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        file_cache = true
        python_path = "python
    "#);
    assert!(cfg.err().to_lowercase().contains("toml"));
}

#[test]
fn malformed_config_missing_required_fields_defaults_to_default() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");

    // Missing 'name' field → defaults to "default".
    write_odools(&ws, r#"
        [[config]]
        file_cache = true
    "#);
    assert!(cfg.ok().0.contains_key("default"));

    // Completely empty config → still yields a "default" entry.
    write_odools(&ws, "");
    assert!(cfg.ok().0.contains_key("default"));
}

#[test]
fn auto_refresh_delay_clamped_to_bounds() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");

    // Below minimum → clamps to 1000.
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 100
    "#);
    assert_eq!(cfg.default().auto_refresh_delay(), 1000);

    // Above maximum → clamps to 15000.
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 99999
    "#);
    assert_eq!(cfg.default().auto_refresh_delay(), 15000);

    // Within bounds → kept verbatim.
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 1234
    "#);
    assert_eq!(cfg.default().auto_refresh_delay(), 1234);
}

#[test]
fn config_reference_defaults_match_wiki() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(&ws, r#"[[config]]
name = "default"
"#);
    let config = cfg.default();

    // Wiki "Configuration reference" table defaults.
    assert!(config.file_cache());
    assert_eq!(config.diag_missing_imports(),
        odoo_ls_server::core::config::DiagMissingImportsMode::All);
    assert!(config.ac_filter_model_names());
    assert_eq!(config.auto_refresh_delay(), 1000);
    assert!(!config.no_typeshed_stubs());
    assert!(!config.is_javascript_disabled());
}

#[test]
fn config_reference_all_keys_settable() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");

    let stubs_dir = cfg.dir("my_stubs");
    let stdlib_dir = cfg.dir("my_stdlib");
    let addons = cfg.dir("my_addons");
    make_addon(&addons, "test_mod");
    let community = cfg.dir("community");
    make_odoo(&community);

    write_odools(&ws, &format!(
        r#"[[config]]
name = "full_config"
odoo_path = "{odoo}"
addons_paths = ["{addons}"]
addons_merge = "override"
python_path = "python3"
stdlib = "{stdlib}"
additional_stubs = ["{stubs}"]
additional_stubs_merge = "merge"
additional_languages = ["fr_BE", "es"]
file_cache = false
diag_missing_imports = "only_odoo"
ac_filter_model_names = false
auto_refresh_delay = 5000
no_typeshed_stubs = true
disable_javascript = true
"#,
        odoo = canonicalized(community.path()),
        addons = canonicalized(addons.path()),
        stdlib = canonicalized(stdlib_dir.path()),
        stubs = canonicalized(stubs_dir.path()),
    ));

    let config = cfg.entry("full_config");
    assert_eq!(config.odoo_path().as_ref().unwrap(), &canonicalized(community.path()));
    assert!(config.addons_paths().contains(&canonicalized(addons.path())));
    assert_eq!(config.python_path(), "python3");
    assert_eq!(config.stdlib(), canonicalized(stdlib_dir.path()));
    assert!(config.additional_stubs().contains(&canonicalized(stubs_dir.path())));
    assert!(config.additional_languages().contains("fr_BE"));
    assert!(config.additional_languages().contains("es"));
    assert!(!config.file_cache());
    assert_eq!(config.diag_missing_imports(),
        odoo_ls_server::core::config::DiagMissingImportsMode::OnlyOdoo);
    assert!(!config.ac_filter_model_names());
    assert_eq!(config.auto_refresh_delay(), 5000);
    assert!(config.no_typeshed_stubs());
    assert!(config.is_javascript_disabled());
}

#[test]
fn diag_missing_imports_values() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");

    for (toml_val, expected) in [
        ("all", odoo_ls_server::core::config::DiagMissingImportsMode::All),
        ("only_odoo", odoo_ls_server::core::config::DiagMissingImportsMode::OnlyOdoo),
        ("none", odoo_ls_server::core::config::DiagMissingImportsMode::None),
    ] {
        write_odools(&ws, &format!(r#"[[config]]
name = "default"
diag_missing_imports = "{toml_val}"
"#));
        assert_eq!(cfg.default().diag_missing_imports(), expected,
            "diag_missing_imports = \"{toml_val}\" should produce {expected:?}");
    }
}

#[test]
fn additional_languages_merged_across_files_with_base_expansion() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");

    // Parent declares fr_WA; workspace declares nl_WV. Always merged (no override).
    write_odools(&cfg.temp, r#"
        [[config]]
        name = "default"
        additional_languages = ["fr_WA"]
    "#);
    write_odools(&ws, r#"
        [[config]]
        name = "default"
        additional_languages = ["nl_WV"]
    "#);

    let langs = cfg.default().additional_languages();
    assert!(langs.contains("fr_WA"), "fr_WA should be present: {langs:?}");
    assert!(langs.contains("nl_WV"), "nl_WV should be present: {langs:?}");
    // Base languages are auto-expanded from the regional variants.
    assert!(langs.contains("fr"), "base 'fr' expanded from fr_WA: {langs:?}");
    assert!(langs.contains("nl"), "base 'nl' expanded from nl_WV: {langs:?}");
}

#[test]
fn additional_stubs_basic_path_resolution() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");
    let stubs_dir = cfg.dir("stubs1");
    stubs_dir.child("package.pyi").touch().unwrap();

    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        additional_stubs = ["{}"]
    "#, canonicalized(stubs_dir.path())));

    assert!(cfg.default().additional_stubs().contains(&canonicalized(stubs_dir.path())));
}

#[test]
fn additional_stubs_template_variable_resolution() {
    let mut cfg = Cfg::new();
    let ws1 = cfg.ws("ws1");
    cfg.ws("ws2");

    let stubs1 = cfg.dir("stubs1");
    stubs1.child("stub1.pyi").touch().unwrap();
    let stubs2 = cfg.dir("stubs2");
    stubs2.child("stub2.pyi").touch().unwrap();

    write_odools(&ws1, &format!(r#"
        [[config]]
        name = "default"
        additional_stubs = [
            "{}",
            "${{workspaceFolder:ws2}}/../stubs2",
        ]
    "#, canonicalized(stubs1.path())));

    let stubs = cfg.default().additional_stubs();
    assert!(stubs.contains(&canonicalized(stubs1.path())));
    assert!(stubs.contains(&canonicalized(stubs2.path())));
}

#[test]
fn additional_stubs_relative_path_canonicalized() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");
    let stubs_dir = ws.child("my_stubs");
    stubs_dir.create_dir_all().unwrap();
    stubs_dir.child("module.pyi").touch().unwrap();

    write_odools(&ws, r#"
        [[config]]
        name = "default"
        additional_stubs = ["my_stubs"]
    "#);

    // Relative path is canonicalized to an absolute path.
    assert!(cfg.default().additional_stubs().contains(&canonicalized(stubs_dir.path())));
}

#[test]
fn stdlib_template_variable_resolution() {
    let mut cfg = Cfg::new();
    let ws1 = cfg.ws("ws1");
    cfg.ws("ws2");

    let stdlib2 = cfg.dir("stdlib2");

    write_odools(&ws1, r#"
        [[config]]
        name = "default"
        stdlib = "${workspaceFolder:ws2}/../stdlib2"
    "#);

    // stdlib resolves via template variable + relative path.
    assert_eq!(cfg.default().stdlib(), canonicalized(stdlib2.path()));
}

#[test]
fn ambiguous_workspace_name_pattern_ignored_but_general_resolves() {
    let mut cfg = Cfg::new();
    // Two distinct folders sharing the same name "duplicate".
    let ws1 = cfg.ws_named("duplicate", "ws1");
    make_addon(&ws1, "my_module");
    let ws2 = cfg.ws_named("duplicate", "ws2");
    make_addon(&ws2, "my_module");

    // Ambiguous ${workspaceFolder:duplicate} → resolves to nothing.
    write_odools(&cfg.temp, r#"
        [[config]]
        name = "default"
        addons_paths = ["${workspaceFolder:duplicate}"]
    "#);
    let addons = cfg.default().addons_paths();
    assert!(!addons.contains(&canonicalized(ws1.path())));
    assert!(!addons.contains(&canonicalized(ws2.path())));
    assert!(addons.is_empty());

    // General ${workspaceFolder} → both resolve.
    write_odools(&cfg.temp, r#"
        [[config]]
        name = "default"
        addons_paths = ["${workspaceFolder}"]
    "#);
    let addons = cfg.default().addons_paths();
    assert!(addons.contains(&canonicalized(ws1.path())));
    assert!(addons.contains(&canonicalized(ws2.path())));
}

#[test]
fn nonexistent_workspace_folder_name_skipped() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    make_addon(&ws, "mod1");

    write_odools(&ws, r#"[[config]]
name = "default"
addons_paths = ["${workspaceFolder:nonexistent}"]
"#);

    // A nonexistent workspace name produces no addon path.
    assert!(cfg.default().addons_paths().is_empty());
}

#[test]
fn invalid_scalar_path_excluded_from_runtime_and_surfaced_in_view() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");

    // Exists, but not a valid Odoo checkout (no odoo/release.py).
    let fake_odoo = cfg.dir("fake_odoo");
    fake_odoo.child("odoo").create_dir_all().unwrap();

    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        odoo_path = "{}"
    "#, canonicalized(fake_odoo.path())));

    let (config_map, config_file) = cfg.ok();

    // Runtime: the invalid scalar is not used (no fallback available here).
    assert!(config_map.get("default").unwrap().odoo_path().is_none(),
        "an invalid odoo_path must not be used at runtime");

    // View: still surfaced, with a non-empty info note.
    let json = serde_json::to_value(&config_file).unwrap();
    let config_arr = json.get("config").unwrap().as_array().unwrap();
    let root = config_arr.iter().find(|c| c.get("name").unwrap() == "default").unwrap();
    let odoo_path = root.get("odoo_path").expect("rejected odoo_path should still appear in the view");
    assert!(!odoo_path.get("info").unwrap().as_str().unwrap().is_empty(),
        "a rejected path must carry a non-empty info note");
}

#[test]
fn invalid_list_item_excluded_from_runtime_and_surfaced_in_view() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");

    // A valid addon dir (holds a module) ...
    let good = ws.child("good_addons");
    good.create_dir_all().unwrap();
    good.child("mod").create_dir_all().unwrap();
    good.child("mod").child("__manifest__.py").touch().unwrap();
    // ... and a dir that exists but is not a valid addon path (no module inside).
    let bad = ws.child("bad_addons");
    bad.create_dir_all().unwrap();

    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        addons_paths = ["{}", "{}"]
    "#, canonicalized(good.path()), canonicalized(bad.path())));

    let (config_map, config_file) = cfg.ok();

    // Runtime: only the valid entry is used; the invalid one is excluded.
    let addons = config_map.get("default").unwrap().addons_paths();
    assert!(addons.contains(&canonicalized(good.path())));
    assert!(!addons.contains(&canonicalized(bad.path())),
        "an invalid addons entry must not be used at runtime");

    // View: the invalid entry still appears, with a non-empty info note.
    let json = serde_json::to_value(&config_file).unwrap();
    let config_arr = json.get("config").unwrap().as_array().unwrap();
    let root = config_arr.iter().find(|c| c.get("name").unwrap() == "default").unwrap();
    let arr = root.get("addons_paths").unwrap().as_array().unwrap();
    let bad_path = canonicalized(bad.path());
    let bad_entry = arr.iter()
        .find(|e| e.get("value").and_then(|v| v.as_str()) == Some(bad_path.as_str()))
        .expect("rejected addons entry should appear in the view");
    assert!(!bad_entry.get("info").unwrap().as_str().unwrap().is_empty(),
        "a rejected list entry must carry a failure note");
}

#[test]
fn invalid_additional_stubs_item_excluded_from_runtime() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws1");

    let stubs_dir = cfg.dir("stubs");
    stubs_dir.child("package.pyi").touch().unwrap();

    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        additional_stubs = ["{}", "{}"]
    "#, canonicalized(stubs_dir.path()), "/nonexistent/path/to/stubs"));

    // Valid path is used at runtime; the invalid one is excluded.
    let stubs = cfg.default().additional_stubs();
    assert!(stubs.contains(&canonicalized(stubs_dir.path())));
    assert!(!stubs.iter().any(|p| p.contains("nonexistent")));
}

#[test]
fn invalid_odoo_path_inferred_still_shows_rejection() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    // The workspace root is itself a valid Odoo checkout (inference target).
    ws.child("odoo").create_dir_all().unwrap();
    ws.child("odoo").child("release.py").touch().unwrap();

    // An explicit odoo_path that exists but is not a valid checkout.
    let bad = cfg.dir("bad_odoo");

    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        odoo_path = "{}"
    "#, canonicalized(bad.path())));

    let (config_map, config_file) = cfg.ok();

    // Runtime: inferred to the workspace root after the explicit one is rejected.
    assert_eq!(config_map.get("default").unwrap().odoo_path(), Some(canonicalized(ws.path())),
        "odoo_path should be inferred after the explicit one is rejected");

    // View: shows the inferred value, with an info note naming the rejected path.
    let json = serde_json::to_value(&config_file).unwrap();
    let config_arr = json.get("config").unwrap().as_array().unwrap();
    let root = config_arr.iter().find(|c| c.get("name").unwrap() == "default").unwrap();
    let op = root.get("odoo_path").unwrap();
    assert_eq!(op.get("value").and_then(|v| v.as_str()), Some(canonicalized(ws.path()).as_str()));
    let info = op.get("info").unwrap().as_str().unwrap();
    assert!(info.contains(&canonicalized(bad.path())),
        "odoo_path info must note the rejected explicit path, got: {info}");
}

#[test]
fn invalid_python_path_fallback_still_shows_rejection() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");

    let bad_python = "/this/python/does/not/exist";
    write_odools(&ws, &format!(r#"
        [[config]]
        name = "default"
        python_path = "{bad_python}"
    "#));

    let (_config_map, config_file) = cfg.ok();

    let json = serde_json::to_value(&config_file).unwrap();
    let config_arr = json.get("config").unwrap().as_array().unwrap();
    let root = config_arr.iter().find(|c| c.get("name").unwrap() == "default").unwrap();
    let pp = root.get("python_path").unwrap();
    let info = pp.get("info").unwrap().as_str().unwrap();
    assert!(info.contains(bad_python),
        "python_path info must note the rejected explicit value, got: {info}");
}

#[test]
fn needs_restart_identical_config_is_false() {
    assert!(!needs_restart(&ConfigEntry::new(), &ConfigEntry::new()));
}

#[test]
fn needs_restart_true_for_restart_fields() {
    let restart_change = |f: &dyn Fn(&mut ConfigEntry)| {
        let mut new = ConfigEntry::new();
        f(&mut new);
        needs_restart(&ConfigEntry::new(), &new)
    };
    assert!(restart_change(&|c| c.set_str(ConfigKey::OdooPath, S!("/some/odoo"))), "odoo_path");
    assert!(restart_change(&|c| c.set_str(ConfigKey::PythonPath, S!("python-other"))), "python_path");
    assert!(restart_change(&|c| c.set_bool(ConfigKey::NoTypeshedStubs, true)), "no_typeshed_stubs");
    assert!(restart_change(&|c| c.set_bool(ConfigKey::DisableJavascript, true)), "disable_javascript");
    assert!(restart_change(&|c| c.set_str(ConfigKey::Stdlib, S!("/some/stdlib"))), "stdlib");
    assert!(
        restart_change(&|c| c.extend_string_list(ConfigKey::AddonsPaths, [S!("/some/addons")])),
        "addons_paths"
    );
    assert!(
        restart_change(&|c| c.extend_string_list(ConfigKey::AdditionalStubs, [S!("/some/stubs")])),
        "additional_stubs"
    );
}

#[test]
fn needs_restart_false_for_hot_reload_fields() {
    let hot_change = |f: &dyn Fn(&mut ConfigEntry)| {
        let mut new = ConfigEntry::new();
        f(&mut new);
        needs_restart(&ConfigEntry::new(), &new)
    };
    assert!(!hot_change(&|c| c.set_bool(ConfigKey::FileCache, false)), "file_cache");
    assert!(!hot_change(&|c| c.set_str(ConfigKey::DiagMissingImports, S!("none"))), "diag_missing_imports");
    assert!(!hot_change(&|c| c.set_bool(ConfigKey::AcFilterModelNames, false)), "ac_filter_model_names");
    assert!(!hot_change(&|c| c.set_u64(ConfigKey::AutoRefreshDelay, 5000)), "auto_refresh_delay");
    assert!(
        !hot_change(&|c| c.extend_string_list(ConfigKey::AdditionalLanguages, [S!("fr")])),
        "additional_languages"
    );
}

#[test]
fn config_schema_generates_and_exposes_all_keys() {
    let json = config_json_schema();
    let text = serde_json::to_string(&json).unwrap();

    // Every documented config key must appear in the schema.
    for key in [
        "odoo_path",
        "python_path",
        "addons_paths",
        "addons_merge",
        "additional_stubs",
        "additional_stubs_merge",
        "additional_languages",
        "file_cache",
        "diag_missing_imports",
        "ac_filter_model_names",
        "auto_refresh_delay",
        "no_typeshed_stubs",
        "disable_javascript",
        "stdlib",
        "diagnostic_settings",
        "diagnostic_filters",
        "extends",
        "$version",
        "$base",
    ] {
        assert!(text.contains(key), "schema is missing config key '{key}'");
    }

    // Enum value sets must be present (catches enum-shape regressions).
    for token in ["only_odoo", "merge", "override"] {
        assert!(text.contains(token), "schema is missing enum value '{token}'");
    }
}

/// An unknown config key is ignored (logged), not fatal: a typo or a
/// forward/backward-compat key must not discard the rest of the configuration.
#[test]
fn unknown_config_key_is_ignored() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(&ws, r#"[[config]]
name = "default"
file_cache = false
totally_unknown_key = "whatever"
refresh_mode = "lazy"
"#);

    // Resolution succeeds and the known key still applies.
    let config = cfg.default();
    assert!(!config.file_cache());
}

/// A `config` key of the wrong shape (a table, not an array of tables) is a
/// mistake and must error rather than silently yielding no profiles.
#[test]
fn config_key_must_be_an_array() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(&ws, "[config]\nname = \"default\"\n");
    assert!(cfg.err().contains("array"), "a non-array 'config' must error");
}

/// `$version` is per-source-local: two workspace folders resolving the same
/// profile to different versions must NOT abort cross-workspace resolution
/// (whereas a genuine scalar like `file_cache` still conflicts).
#[test]
fn cross_workspace_different_versions_do_not_conflict() {
    let mut c = Cfg::new();
    let ws1 = c.ws("ws1");
    let ws2 = c.ws("ws2");
    write_odools(&ws1, "[[config]]\nname = \"default\"\n\"$version\" = \"17.0\"\n");
    write_odools(&ws2, "[[config]]\nname = \"default\"\n\"$version\" = \"18.0\"\n");

    let (map, _) = c.ok();
    assert!(map.contains_key("default"), "differing $version must not abort resolution");
}

/// A standalone config file with no workspace folders still yields a `default`
/// profile (matching the per-workspace path), even when it defines only named
/// profiles.
#[test]
fn config_file_only_yields_default_profile() {
    let mut c = Cfg::new();
    let ext = c.dir("cfg").child("external.toml");
    ext.write_str("[[config]]\nname = \"custom\"\nfile_cache = false\n").unwrap();
    c.with_config_file(canonicalized(ext.path()));

    let (map, _) = c.ok();
    assert!(map.contains_key("default"), "a default profile is always present");
    assert!(map.contains_key("custom"));
}

/// Restart contract: an unset list and an explicit empty list are equivalent and
/// must not trigger a spurious restart.
#[test]
fn needs_restart_false_for_unset_vs_empty_list() {
    let old = ConfigEntry::new();
    let mut new = ConfigEntry::new();
    new.set_string_list(ConfigKey::AddonsPaths, std::iter::empty::<String>());
    assert!(
        !needs_restart(&old, &new),
        "an unset list vs an explicit empty list must not trigger a restart"
    );
}

/// The config panel HTML is built from user-controlled values: they must be
/// HTML-escaped to prevent script injection into the VS Code webview.
#[test]
fn config_view_html_escapes_user_values() {
    let mut cfg = Cfg::new();
    let ws = cfg.ws("ws");
    write_odools(
        &ws,
        r#"[[config]]
name = "default"
additional_languages = ["<script>alert(1)</script>"]
"#,
    );

    let html = cfg.view().to_html_string();
    assert!(
        !html.contains("<script>alert(1)</script>"),
        "raw script must not appear unescaped in the panel HTML"
    );
    assert!(html.contains("&lt;script&gt;"), "the value should be HTML-escaped");
}
