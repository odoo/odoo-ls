use std::{ffi::OsStr, fs, path::{Path, PathBuf}};

#[macro_export]
macro_rules! S {
    ($x: expr) => {
        String::from($x)
    };
}

#[cfg(target_os = "windows")]
pub fn is_file_cs(path: String) -> bool {
    let mut p = Path::new(&path);
    if p.exists() {
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "unix"))]
pub fn is_file_cs(path: String) -> bool {
    let p = Path::new(&path);
    if p.exists() {
        match p.file_name().and_then(OsStr::to_str) {
            Some(name) => name == path,
            None => false
        }
    } else {
        false
    }
}

pub fn is_dir_cs(path: String) -> bool {
    match fs::canonicalize(path) {
        Ok(canonical_path) => {
            return fs::metadata(canonical_path).unwrap().is_dir()
        }
        Err(_err) => {
            return false;
        }
    }
}

//TODO use it?
pub fn is_symlink_cs(path: String) -> bool {
    match fs::canonicalize(path) {
        Ok(canonical_path) => {
            return fs::metadata(canonical_path).unwrap().is_symlink()
        }
        Err(_err) => {
            return false;
        }
    }
}