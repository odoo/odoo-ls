use std::fs;

#[macro_export]
macro_rules! S {
    ($x: expr) => {
        String::from($x)
    };
}

#[derive(Debug)]
pub enum Either<T1, T2> {
    Left(T1),
    Right(T2),
}

impl <T1, T2> Either<T1, T2> {
    pub fn left(&self) -> Option<&T1> {
        match self {
            Either::Left(left) => Some(left),
            _ => None
        }
    }

    pub fn right(&self) -> Option<&T2> {
        match self {
            Either::Right(right) => Some(right),
            _ => None
        }
    }
}

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