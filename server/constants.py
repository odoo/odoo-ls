from enum import Enum

EXTENSION_NAME = "Odoo"
EXTENSION_VERSION = "0.1.0"

CMD_COUNT_DOWN_BLOCKING = 'countDownBlocking'
CMD_COUNT_DOWN_NON_BLOCKING = 'countDownNonBlocking'
CMD_PROGRESS = 'progress'
CMD_REGISTER_COMPLETIONS = 'registerCompletions'
CMD_SHOW_CONFIGURATION_ASYNC = 'showConfigurationAsync'
CMD_SHOW_CONFIGURATION_CALLBACK = 'showConfigurationCallback'
CMD_SHOW_CONFIGURATION_THREAD = 'showConfigurationThread'
CMD_UNREGISTER_COMPLETIONS = 'unregisterCompletions'

CONFIGURATION_SECTION = 'Odoo'

#DEBUG PARAMETERS

DEBUG_BUILD_ONLY_BASE = False
DEBUG_ARCH_BUILDER = True
DEBUG_ARCH_EVAL = True
DEBUG_ODOO_BUILDER = True
DEBUG_VALIDATION = True
DEBUG_MEMORY = True
DEBUG_REBUILD = True

class SymType(Enum):
    DIRTY     = -1,
    ROOT      = 0,
    NAMESPACE = 1,
    PACKAGE   = 2,
    FILE      = 3,
    COMPILED  = 4,
    CLASS     = 5,
    FUNCTION  = 6,
    VARIABLE  = 7,
    PRIMITIVE = 8

    def __str__(self):
        return self.name

class BuildSteps(Enum):
    ARCH        = 0,
    ARCH_EVAL   = 1,
    ODOO        = 2,
    VALIDATION  = 3

BUILT_IN_LIBS = ["string", "re", "difflib", "textwrap", "unicodedata", "stringprep", "readline", "rlcompleter",
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
"pipes", "smtpd", "sndhdr", "spwd", "sunau", "telnetlib", "uu", "xdrlib", "struct", "codecs"]