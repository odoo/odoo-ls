#![allow(non_camel_case_types)]
use core::fmt;


pub const EXTENSION_NAME: &str = "Odoo";
pub const EXTENSION_VERSION: &str = "1.0.1";

pub const DEBUG_ODOO_BUILDER: bool = false;
pub const DEBUG_MEMORY: bool = false;
pub const DEBUG_THREADS: bool = false;
pub const DEBUG_STEPS: bool = false;
pub const DEBUG_STEPS_ONLY_INTERNAL: bool = true;
pub const DEBUG_REBUILD_NOW: bool = false;
pub const DEBUG_BORROW_GUARDS: bool = false;

pub type Tree = (Vec<OYarn>, Vec<OYarn>);

//type DebugYarn = String;

#[macro_export]
#[cfg(not(all(feature="debug_yarn", debug_assertions)))]
macro_rules! oyarn {
    ($($args:tt)*) => {
        byteyarn::yarn!($($args)*)
    };
}
#[macro_export]
#[cfg(all(feature="debug_yarn", debug_assertions))]
macro_rules! oyarn {
    ($($args:tt)*) => {
        format!($($args)*)
    };
}

#[cfg(all(feature="debug_yarn", debug_assertions))]
pub type OYarn = String;
#[cfg(not(all(feature="debug_yarn", debug_assertions)))]
use byteyarn::Yarn;
#[cfg(not(all(feature="debug_yarn", debug_assertions)))]
pub type OYarn = Yarn;

pub fn tree(a: Vec<&str>, b: Vec<&str>) -> Tree {
    (a.iter().map(|x| oyarn!("{}", *x)).collect(), b.iter().map(|x| oyarn!("{}", *x)).collect())
}

pub fn flatten_tree(tree: &Tree) -> Vec<OYarn> {
    [tree.0.clone(), tree.1.clone()].concat()
}


#[derive(Debug, Eq, Hash, PartialEq, Copy, Clone)]
pub enum PackageType{
    PYTHON_PACKAGE,
    MODULE
}

#[derive(Debug, Eq, Hash, PartialEq, Copy, Clone)]
pub enum SymType{
    ROOT,
    DISK_DIR,
    NAMESPACE,
    PACKAGE(PackageType),
    FILE,
    COMPILED,
    VARIABLE,
    CLASS,
    FUNCTION,
    XML_FILE,
    CSV_FILE,
}

impl fmt::Display for SymType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Copy, Clone)]
pub enum BuildSteps {
    SYNTAX     = -1, //can't be 0, because others should be able to be used as vec index
    ARCH       = 0,
    ARCH_EVAL  = 1,
    VALIDATION = 2,
}

impl From<i32> for BuildSteps {
    fn from(value: i32) -> Self {
        match value {
            -1 => BuildSteps::SYNTAX,
            0 => BuildSteps::ARCH,
            1 => BuildSteps::ARCH_EVAL,
            2 => BuildSteps::VALIDATION,
            _ => panic!("Invalid value for BuildSteps: {}", value),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum BuildStatus {
    PENDING,
    IN_PROGRESS,
    INVALID,
    DONE
}

pub const BUILT_IN_LIBS: &[&str]  = &["string", "re", "difflib", "textwrap", "unicodedata", "stringprep", "readline", "rlcompleter",
"datetime", "zoneinfo", "calendar", "collections", "heapq", "bisect", "array", "weakref", "types", "copy", "pprint",
"reprlib", "enum", "graphlib", "numbers", "math", "cmath", "decimal", "fractions", "random", "statistics", "itertools",
"functools", "operator", "pathlib", "fileinput", "stat", "filecmp", "tempfile", "glob", "fnmatch", "linecache",
"shutil", "pickle", "copyreg", "shelve", "marshal", "dbm", "sqlite3", "zlib", "gzip", "bz2", "lzma", "zipfile",
"tarfile", "csv", "configparser", "tomllib", "netrc", "plistlib", "hashlib", "hmac", "secrets", "os", "io", "time",
"argparse", "getopt", "logging", "getpass", "curses", "platform", "errno", "ctypes", "threading", "multiprocessing",
"concurrent", "subprocess", "sched", "queue", "contextvars", "_thread", "asyncio", "socket", "ssl", "select",
"selectors", "signal", "mmap", "email", "json", "mailbox", "mimetypes", "base64", "binascii", "quopri", "html",
"xml", "webbrowser", "wsgiref", "urllib", "http", "ftplib", "poplib", "imaplib", "smtplib", "uuid", "socketserver",
"xmlrpc", "ipaddress", "wave", "colorsys", "gettext", "locale", "turtle", "cmd", "shlex", "tkinter", "IDLE",
"typing", "pydoc", "doctest", "unittest", "2to3", "test", "bdb", "faulthandler", "pdb", "timeit", "trace",
"tracemalloc", "distutils", "ensurepip", "venv", "zipapp", "sys", "sysconfig", "builtins", "__main__", "warnings",
"dataclasses", "contextlib", "abc", "atexit", "traceback", "__future__", "gc", "inspect", "site", "code", "codeop",
"zipimport", "pkgutil", "modulefinder", "runpy", "importlib", "ast", "symtable", "token", "keyword", "tokenize",
"tabnanny", "pyclbr", "py_compile", "compileall", "dis", "pickletools", "msvcrt", "winreg", "winsound", "posix",
"pwd", "grp", "termios", "tty", "pty", "fcntl", "resource", "syslog", "aifc", "asynchat", "asyncore", "audioop",
"cgi", "cgitb", "chunk", "crypt", "imghdr", "imp", "mailcap", "msilib", "nis", "nntplib", "optparse", "ossaudiodev",
"pipes", "smtpd", "sndhdr", "spwd", "sunau", "telnetlib", "uu", "xdrlib", "struct", "codecs"];

pub const CONFIG_WIKI_URL: &str = "https://github.com/odoo/odoo-ls/wiki/Configuration-files";

use std::sync::LazyLock;

/// True if the minor part of EXTENSION_VERSION is even, false otherwise.
pub static IS_RELEASE: LazyLock<bool> = LazyLock::new(|| {
    let parts: Vec<&str> = EXTENSION_VERSION.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    if let Ok(minor) = parts[1].parse::<u32>() {
        return minor % 2 == 0;
    }
    false
});
