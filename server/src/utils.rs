use std::fs;

pub fn is_file_cs(path: String) -> bool {
    match fs::canonicalize(path) {
        Ok(canonical_path) => {
            return fs::metadata(canonical_path).unwrap().is_file()
        }
        Err(_err) => {
            return false;
        }
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