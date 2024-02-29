const EXTENSION_NAME: &str = "Odoo";
const EXTENSION_VERSION: &str = "0.2.4";

const DEBUG_BUILD_ONLY_BASE: bool = false;
const DEBUG_ARCH_BUILDER: bool = false;
const DEBUG_ARCH_EVAL: bool = false;
const DEBUG_ODOO_BUILDER: bool = false;
const DEBUG_VALIDATION: bool = false;
const DEBUG_MEMORY: bool = false;
const DEBUG_REBUILD: bool = false;

pub type Tree = (Vec<String>, Vec<String>);

pub fn tree(a: Vec<&str>, b: Vec<&str>) -> Tree {
    (a.iter().map(|x| x.to_string()).collect(), b.iter().map(|x| x.to_string()).collect())
}

#[derive(Debug, Eq, Hash, PartialEq, Copy, Clone)]
pub enum SymType{
    DIRTY,
    ROOT,
    NAMESPACE,
    PACKAGE,
    FILE,
    COMPILED,
    CLASS,
    FUNCTION,
    VARIABLE,
    CONSTANT,
}

impl SymType {
    pub fn is_instance(sym_type: &SymType) -> bool {
        match sym_type {
            SymType::VARIABLE | SymType::CONSTANT => true,
            _ => false,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Copy, Clone)]
pub enum BuildSteps {
    SYNTAX     = -1, //can't be 0, because others should be able to be used as vec index
    ARCH       = 0,
    ARCH_EVAL  = 1,
    ODOO       = 2,
    VALIDATION = 3,
}

#[derive(Debug, Eq, PartialEq)]
pub enum BuildStatus {
    PENDING,
    IN_PROGRESS,
    DONE
}

const BUILT_IN_LIBS: &[&str]  = &["string", "re", "difflib", "textwrap", "unicodedata", "stringprep", "readline", "rlcompleter",
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