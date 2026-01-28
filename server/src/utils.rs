use crate::core::file_mgr::legacy_unc_paths;
use path_slash::{PathBufExt, PathExt};
use regex::Regex;
use ruff_text_size::TextSize;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::{collections::HashMap, fs::{self, DirEntry}, path::{Path, PathBuf}, str::FromStr, sync::LazyLock};

use crate::{constants::Tree, oyarn};

static TEMPLATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([^}]+)\}").unwrap()
});
static HOME_DIR: LazyLock<Option<String>> = LazyLock::new(|| dirs::home_dir().map(|buf| buf.sanitize()));

#[macro_export]
macro_rules! S {
    ($x: expr) => {
        String::from($x)
    };
}

#[macro_export]
macro_rules! Sy {
    ($x: expr) => {
        OYarn::from($x)
    };
}

pub fn get_python_command() -> Option<String> {
    for cmd in &["python3", "python"] {
        if let Ok(output) = Command::new(cmd).arg("--version").output() {
            if output.status.success() {
                return Some(S!(*cmd));
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
pub fn is_file_cs(path: String) -> bool {
    let mut p = Path::new(&path);
    if p.exists() && p.is_file() {
        while p.parent().is_some() {
            let mut found = false;
            if let Ok(entries) = fs::read_dir(p.parent().unwrap()) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        if entry.file_name() == p.components().last().unwrap().as_os_str() {
                            found = true;
                            break;
                        }
                    }
                }
            }
            if !found {
                return false;
            }
            p = p.parent().unwrap();
        }
        return true;
    }
    false
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn is_file_cs(path: String) -> bool {
    let p = Path::new(&path);
    p.exists() && p.is_file()
}

#[cfg(target_os = "windows")]
pub fn is_dir_cs(path: String) -> bool {
    let mut p = Path::new(&path);
    if p.exists() && p.is_dir() {
        while p.parent().is_some() {
            let mut found = false;
            if let Ok(entries) = fs::read_dir(p.parent().unwrap()) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        if entry.file_name() == p.components().last().unwrap().as_os_str() {
                            found = true;
                            break;
                        }
                    }
                }
            }
            if !found {
                return false;
            }
            p = p.parent().unwrap();
        }
        return true;
    }
    false
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn is_dir_cs(path: String) -> bool {
    let p = Path::new(&path);
    p.exists() && p.is_dir()
}

//TODO use it?
pub fn is_symlink_cs(path: String) -> bool {
    match fs::canonicalize(path) {
        Ok(canonical_path) => {
            fs::metadata(canonical_path).unwrap().is_symlink()
        }
        Err(_err) => {
            false
        }
    }
}

pub fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| {
        s.split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };

    let mut va = parse(a);
    let mut vb = parse(b);

    // Pad shorter version with zeroes to match length
    let max_len = va.len().max(vb.len());
    va.resize(max_len, 0);
    vb.resize(max_len, 0);

    va.cmp(&vb)
}

pub trait ToFilePath {
    fn to_file_path(&self) -> Result<PathBuf, ()>;
}

impl ToFilePath for lsp_types::Uri {
    fn to_file_path(&self) -> Result<PathBuf, ()> {
        let s = self.as_str();
        // Detect legacy UNC path (file:////)
        if s.starts_with("file:////") {
            legacy_unc_paths().store(true, Ordering::Relaxed);
        }
        let str_repr = s.replace("file:////", "file://");
        let url = url::Url::from_str(&str_repr).map_err(|_| ())?;
        url.to_file_path()
    }
}

pub trait PathSanitizer {
    fn sanitize(&self) -> String;
    fn to_tree(&self) -> Tree;
    fn to_tree_path(&self) -> PathBuf;
}

impl PathSanitizer for PathBuf {

    fn sanitize(&self) -> String {
        #[cfg(windows)]
        {
            let mut path = self.to_slash_lossy().to_string();
            // check if path begins with //?/ if yes remove it
            // to handle extended-length path prefix
            // https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
            if path.starts_with("\\\\?\\") {
                path = path[4..].to_string();
            }
            // Check if path begin with a letter + ':'
            if path.len() > 2 && path.chars().nth(1) == Some(':') {
                let disk_letter = path.chars().next().unwrap().to_ascii_lowercase();
                path.replace_range(0..1, &disk_letter.to_string());
            }
            return path;
        }

        #[cfg(not(windows))]
        return self.to_slash_lossy().to_string();
    }

    /// Convert the path to a tree structure.
    fn to_tree(&self) -> Tree {
        let mut tree = (vec![], vec![]);
        self.components().for_each(|c| {
            tree.0.push(oyarn!("{}", c.as_os_str().to_str().unwrap().replace(".py", "").replace(".pyi", "")));
        });
        if matches!(tree.0.last().map(|s| s.as_str()), Some("__init__" | "__manifest__")) {
            tree.0.pop();
        }
        tree
    }

    /// Convert the path to a path valid for the tree structure (without __init__.py or __manifest__.py).
    fn to_tree_path(&self) -> PathBuf {
        if let Some(file_name) = self.file_name() {
            if file_name.to_str().unwrap() == "__init__.py" || file_name.to_str().unwrap() == "__manifest__.py" {
                return self.parent().unwrap().to_path_buf();
            }
        }
        self.clone()
    }
}

impl PathSanitizer for Path {

    fn sanitize(&self) -> String {
        #[cfg(windows)]
        {
            let mut path = self.to_slash_lossy().to_string();
            if path.starts_with("\\\\?\\") {
                path = path[4..].to_string();
            }
            // Check if path begin with a letter + ':'
            if path.len() > 2 && path.chars().nth(1) == Some(':') {
                let disk_letter = path.chars().next().unwrap().to_ascii_lowercase();
                path.replace_range(0..1, &disk_letter.to_string());
            }
            return path;
        }

        #[cfg(not(windows))]
        return self.to_slash_lossy().to_string();
    }

    fn to_tree(&self) -> Tree {
        let mut tree = (vec![], vec![]);
        self.components().for_each(|c| {
            tree.0.push(oyarn!("{}", c.as_os_str().to_str().unwrap().replace(".py", "").replace(".pyi", "")));
        });
        if matches!(tree.0.last().map(|s| s.as_str()), Some("__init__" | "__manifest__")) {
            tree.0.pop();
        }
        tree
    }

    /// Convert the path to a path valid for the tree structure (without __init__.py or __manifest__.py).
    fn to_tree_path(&self) -> PathBuf {
        if let Some(file_name) = self.file_name() {
            if file_name.to_str().unwrap() == "__init__.py" || file_name.to_str().unwrap() == "__manifest__.py" {
                return self.parent().unwrap().to_path_buf();
            }
        }
        self.to_path_buf()
    }
}

pub trait MaxTextSize {
    const MAX: TextSize;
}

impl MaxTextSize for TextSize {
    const MAX: TextSize = TextSize::new(u32::MAX);
}

pub fn has_template(template: &str) -> bool {
    TEMPLATE_REGEX.is_match(template)
}

pub fn fill_template(template: &str, vars: &HashMap<String, String>) -> Result<String, String> {
    let mut invalid = None;

    let result = TEMPLATE_REGEX.replace_all(template, |captures: &regex::Captures| -> String{
        let key = captures[1].to_string();
        if let Some(value) = vars.get(&key) {
            value.clone()
        } else {
            invalid = Some(format!("Invalid key ({}) in pattern", key));
            S!("")
        }
    });
    match invalid {
        Some(err) => Err(err),
        None => Ok(S!(result)),
    }
}


pub fn build_pattern_map(ws_folders: &HashMap<String, String>) -> HashMap<String, String> {
    // TODO: Maybe cache this
    let mut pattern_map = HashMap::new();
    if let Some(home_dir) = HOME_DIR.as_ref() {
        pattern_map.insert(S!("userHome"), home_dir.clone());
    }
    for (ws_name, ws_path) in ws_folders.iter(){
        pattern_map.insert(format!("workspaceFolder:{}", ws_name.clone()), ws_path.clone());
    }
    pattern_map
}


/// Fill the template with the given pattern map.
/// While also checking it with the predicate function.
/// pass `|_| true` to skip the predicate check.
/// Currently, only the workspaceFolder[:workspace_name] and userHome variables are supported.
pub fn fill_validate_path<F, P>(ws_folders: &HashMap<String, String>, workspace_name: Option<&String>, template: &str, predicate: F, var_map: HashMap<String, String>, parent_path: P) -> Result<String, String>
where
    F: Fn(&String) -> bool,
    P: AsRef<Path>
{
        let mut pattern_map: HashMap<String, String> = build_pattern_map(ws_folders).into_iter().chain(var_map.into_iter()).collect();
        if let Some(path) = workspace_name.and_then(|name| ws_folders.get(name)) {
            pattern_map.insert(S!("workspaceFolder"), path.clone());
        }
        let path = fill_template(template, &pattern_map)?;
        if predicate(&path) {
            return Ok(path);
        }
        // Attempt to convert the path to an absolute path
        if let Ok(abs_path) = std::fs::canonicalize(parent_path.as_ref().join(&path)) {
            let abs_path    = abs_path.sanitize();
            if predicate(&abs_path) {
                return Ok(abs_path);
            }
        }
        Err(format!("Failed to fill and validate path: {} from template {}", path, template))
    }

fn is_really_module(directory_path: &str, entry: &DirEntry) -> bool {
    let module_name = entry.file_name();
    let full_path = Path::new(directory_path).join(module_name).join("__manifest__.py");

    // Check if the file exists and is a regular file
    full_path.exists() && full_path.is_file()
}

pub fn is_addon_path(directory_path: &String) -> bool {
    fs::read_dir(directory_path)
    .into_iter()
    .flatten()
    .flatten()
    .any(|entry| is_really_module(directory_path, &entry))
}

pub fn is_odoo_path(directory_path: &String) -> bool {
    let odoo_release_path = Path::new(directory_path).join("odoo").join("release.py");
    odoo_release_path.exists() && odoo_release_path.is_file()
}

pub fn is_python_path(path: &String) -> bool {
    match Command::new(path).arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub fn string_fuzzy_contains(string: &str, pattern: &str) -> bool {
    let mut pattern_char_iter = pattern.chars();
    let mut pattern_char = match pattern_char_iter.next() {
        Some(c) => c.to_ascii_lowercase(),
        None => return true,
    };
    for char in string.chars() {
        if char.to_ascii_lowercase() == pattern_char {
            pattern_char = match pattern_char_iter.next() {
                Some(c) => c.to_ascii_lowercase(),
                None => {
                    return true;
                }
            };
        }
    }
    false
}

#[macro_export]
macro_rules! warn_or_panic {
    ($($arg:tt)*) => {
        if *crate::constants::IS_RELEASE {
            let bt = std::backtrace::Backtrace::force_capture();
            tracing::warn!("{}\nBacktrace:\n{:?}", format!($($arg)*), bt);
        } else {
            panic!($($arg)*);
        }
    }
}
