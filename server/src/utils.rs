use std::{fs, path::{Path, PathBuf}, str::FromStr};
use path_slash::{PathBufExt, PathExt};

#[macro_export]
macro_rules! S {
    ($x: expr) => {
        String::from($x)
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "unix"))]
pub fn is_file_cs(path: String) -> bool {
    let p = Path::new(&path);
    if p.exists() && p.is_file() {
        true
    } else {
        false
    }
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "unix"))]
pub fn is_dir_cs(path: String) -> bool {
    let p = Path::new(&path);
    if p.exists() && p.is_dir() {
        true
    } else {
        false
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
}