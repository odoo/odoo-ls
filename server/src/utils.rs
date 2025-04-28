use std::{fs, path::{Path, PathBuf}, str::FromStr};
use path_slash::{PathBufExt, PathExt};
use ruff_text_size::TextSize;

use crate::{constants::Tree, oyarn};

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

pub trait ToFilePath {
    fn to_file_path(&self) -> Result<PathBuf, ()>;
}

impl ToFilePath for lsp_types::Uri {

    fn to_file_path(&self) -> Result<PathBuf, ()> {
        let url = url::Url::from_str(self.as_str()).map_err(|_| ())?;
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
        let mut path = self.to_slash_lossy().to_string();

        #[cfg(windows)]
        {
            // Check if path begin with a letter + ':'
            if path.len() > 2 && path.chars().nth(1) == Some(':') {
                let disk_letter = path.chars().next().unwrap().to_ascii_lowercase();
                path.replace_range(0..1, &disk_letter.to_string());
            }
        }

        path
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
        let mut path = self.to_slash_lossy().to_string();

        #[cfg(windows)]
        {
            // Check if path begin with a letter + ':'
            if path.len() > 2 && path.chars().nth(1) == Some(':') {
                let disk_letter = path.chars().next().unwrap().to_ascii_lowercase();
                path.replace_range(0..1, &disk_letter.to_string());
            }
        }

        path
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