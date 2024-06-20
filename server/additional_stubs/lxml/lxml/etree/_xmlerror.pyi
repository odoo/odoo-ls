#
# Types for lxml/xmlerror.pxi
#

import enum
from abc import ABCMeta, abstractmethod
from logging import Logger, LoggerAdapter
from typing import Any, Collection, Iterable, Iterator, final, overload

@final
class _LogEntry:
    """Log message entry from an error log

    Attributes
    ----------
    message: str
        the message text
    domain: ErrorDomains
        domain ID
    type: ErrorTypes
        message type ID
    level: ErrorLevels
        log level ID
    line: int
        the line at which the message originated, if applicable
    column: int
        the character column at which the message originated, if applicable
    filename: str, optional
        the name of the file in which the message originated, if applicable
    path: str, optional
        the location in which the error was found, if available"""

    @property
    def domain(self) -> ErrorDomains: ...
    @property
    def type(self) -> ErrorTypes: ...
    @property
    def level(self) -> ErrorLevels: ...
    @property
    def line(self) -> int: ...
    @property
    def column(self) -> int: ...
    @property
    def domain_name(self) -> str: ...
    @property
    def type_name(self) -> str: ...
    @property
    def level_name(self) -> str: ...
    @property
    def message(self) -> str: ...
    @property
    def filename(self) -> str: ...
    @property
    def path(self) -> str | None: ...

class _BaseErrorLog(metaclass=ABCMeta):
    """The base class of all other error logs"""

    @property
    def last_error(self) -> _LogEntry | None: ...
    # copy() method is originally under _BaseErrorLog class. However
    # PyErrorLog overrides it with a dummy version, denoting it
    # shouldn't be used. So move copy() to the only other subclass
    # inherited from _BaseErrorLog, that is _ListErrorLog.
    @abstractmethod
    def receive(self, entry: _LogEntry) -> None: ...

class _ListErrorLog(_BaseErrorLog, Collection[_LogEntry]):
    """Immutable base version of a list based error log"""

    def __init__(
        self,
        entries: list[_LogEntry],
        first_error: _LogEntry | None,
        last_error: _LogEntry | None,
    ) -> None: ...
    def __iter__(self) -> Iterator[_LogEntry]: ...
    def __len__(self) -> int: ...
    def __getitem__(self, __k: int) -> _LogEntry: ...
    def __contains__(self, __o: object) -> bool: ...
    def filter_domains(self, domains: int | Iterable[int]) -> _ListErrorLog: ...
    def filter_types(self, types: int | Iterable[int]) -> _ListErrorLog: ...
    def filter_levels(self, levels: int | Iterable[int]) -> _ListErrorLog: ...
    def filter_from_level(self, level: int) -> _ListErrorLog: ...
    def filter_from_fatals(self) -> _ListErrorLog: ...
    def filter_from_errors(self) -> _ListErrorLog: ...
    def filter_from_warnings(self) -> _ListErrorLog: ...
    def clear(self) -> None: ...
    # Context manager behavior is internal to cython, not usable
    # in python code, so dropped altogether.
    # copy() is originally implemented in _BaseErrorLog, see
    # comment there for more info.
    def copy(self) -> _ListErrorLog: ...  # not Self, subclasses won't survive
    def receive(self, entry: _LogEntry) -> None: ...

# The interaction between _ListErrorLog and _ErrorLog is interesting
def _ErrorLog() -> _ListErrorLog:
    """
    Annotation notes
    ----------------
    `_ErrorLog` is originally a class itself. However, it has very
    special property that it is now annotated as function.

    `_ErrorLog`, when instantiated, generates `_ListErrorLog` object
    instead, and then patches it with extra runtime methods. `Mypy`
    becomes malevolent on any attempt of annotating such behavior.

    Therefore, besides making it a function, all extra properties
    and methods are merged into `_ListErrorLog`, since `_ListErrorLog`
    is seldom instantiated by itself.
    """

class _RotatingErrorLog(_ListErrorLog):
    """Error log that has entry limit and uses FIFO rotation"""

    def __init__(self, max_len: int) -> None: ...

# Maybe there's some sort of hidden commercial version of lxml
# that supports _DomainErrorLog, if such thing exists? Anyway,
# the class in open source lxml is entirely broken and not touched
# since 2006.

class PyErrorLog(_BaseErrorLog):
    """Global error log that connects to the Python stdlib logging package

    Original Docstring
    ------------------
    The constructor accepts an optional logger name or a readily
    instantiated logger instance.

    If you want to change the mapping between libxml2's ErrorLevels and Python
    logging levels, you can modify the level_map dictionary from a subclass.

    The default mapping is::

    ```python
    ErrorLevels.WARNING = logging.WARNING
    ErrorLevels.ERROR   = logging.ERROR
    ErrorLevels.FATAL   = logging.CRITICAL
    ```

    You can also override the method ``receive()`` that takes a LogEntry
    object and calls ``self.log(log_entry, format_string, arg1, arg2, ...)``
    with appropriate data.
    """

    @property
    def level_map(self) -> dict[int, int]: ...
    # Only either one of the 2 args in __init__ is effective;
    # when both are specified, 'logger_name' is ignored
    @overload
    def __init__(
        self,
        logger_name: str | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        *,
        logger: Logger | LoggerAdapter[Any] | None = None,
    ) -> None: ...
    # copy() is disallowed, implementation chooses to fail in a
    # silent way by returning dummy _ListErrorLog. We skip it altogether.
    def log(self, log_entry: _LogEntry, message: str, *args: object) -> None: ...
    def receive(  # pyright: ignore[reportIncompatibleMethodOverride]
        self, log_entry: _LogEntry
    ) -> None: ...

def clear_error_log() -> None: ...
def use_global_python_log(log: PyErrorLog) -> None: ...

# Container for libxml2 constants
# It's overkill to include zillions of constants into type checker;
# and more no-no for updating constants along with each lxml releases
# unless these stubs are bundled with lxml together. So we only do
# minimal enums which do not involve much work. No ErrorTypes. Never.
class ErrorLevels(enum.IntEnum):
    """Error severity level constants

    Annotation notes
    ----------------
    These integer constants sementically fit int enum better, but
    in the end they are just integers. No enum properties and mechanics
    would work on them.
    """

    # fmt: off
    NONE    = 0
    WARNING = 1
    ERROR   = 2
    FATAL   = 3
    # fmt: on

class ErrorDomains(enum.IntEnum):
    """Part of the library that raised error

    Annotation notes
    ----------------
    These integer constants sementically fit int enum better, but
    in the end they are just integers. No enum properties and mechanics
    would work on them.
    """

    # fmt: off
    NONE         = 0
    PARSER       = 1
    TREE         = 2
    NAMESPACE    = 3
    DTD          = 4
    HTML         = 5
    MEMORY       = 6
    OUTPUT       = 7
    IO           = 8
    FTP          = 9
    HTTP         = 10
    XINCLUDE     = 11
    XPATH        = 12
    XPOINTER     = 13
    REGEXP       = 14
    DATATYPE     = 15
    SCHEMASP     = 16
    SCHEMASV     = 17
    RELAXNGP     = 18
    RELAXNGV     = 19
    CATALOG      = 20
    C14N         = 21
    XSLT         = 22
    VALID        = 23
    CHECK        = 24
    WRITER       = 25
    MODULE       = 26
    I18N         = 27
    SCHEMATRONV  = 28
    BUFFER       = 29
    URI          = 30
    # fmt: on

# TODO implement ErrorTypes enum, looks like unavoidable
class ErrorTypes(enum.IntEnum):
    """The actual libxml2 error code

    Annotation notes
    ----------------
    These integer constants sementically fit int enum better, but
    in the end they are just integers. No enum properties and mechanics
    would work on them.

    Because of the vast amount of existing codes, and its ever-increasing
    nature due to newer libxml2 releases, error type constant names
    will not be explicitly listed in stub.
    """

    def __getattr__(self, name: str) -> ErrorTypes: ...

class RelaxNGErrorTypes(enum.IntEnum):
    """RelaxNG specific libxml2 error code

    Annotation notes
    ----------------
    These integer constants sementically fit int enum better, but
    in the end they are just integers. No enum properties and mechanics
    would work on them.
    """

    # fmt: off
    RELAXNG_OK               = 0
    RELAXNG_ERR_MEMORY       = 1
    RELAXNG_ERR_TYPE         = 2
    RELAXNG_ERR_TYPEVAL      = 3
    RELAXNG_ERR_DUPID        = 4
    RELAXNG_ERR_TYPECMP      = 5
    RELAXNG_ERR_NOSTATE      = 6
    RELAXNG_ERR_NODEFINE     = 7
    RELAXNG_ERR_LISTEXTRA    = 8
    RELAXNG_ERR_LISTEMPTY    = 9
    RELAXNG_ERR_INTERNODATA  = 10
    RELAXNG_ERR_INTERSEQ     = 11
    RELAXNG_ERR_INTEREXTRA   = 12
    RELAXNG_ERR_ELEMNAME     = 13
    RELAXNG_ERR_ATTRNAME     = 14
    RELAXNG_ERR_ELEMNONS     = 15
    RELAXNG_ERR_ATTRNONS     = 16
    RELAXNG_ERR_ELEMWRONGNS  = 17
    RELAXNG_ERR_ATTRWRONGNS  = 18
    RELAXNG_ERR_ELEMEXTRANS  = 19
    RELAXNG_ERR_ATTREXTRANS  = 20
    RELAXNG_ERR_ELEMNOTEMPTY = 21
    RELAXNG_ERR_NOELEM       = 22
    RELAXNG_ERR_NOTELEM      = 23
    RELAXNG_ERR_ATTRVALID    = 24
    RELAXNG_ERR_CONTENTVALID = 25
    RELAXNG_ERR_EXTRACONTENT = 26
    RELAXNG_ERR_INVALIDATTR  = 27
    RELAXNG_ERR_DATAELEM     = 28
    RELAXNG_ERR_VALELEM      = 29
    RELAXNG_ERR_LISTELEM     = 30
    RELAXNG_ERR_DATATYPE     = 31
    RELAXNG_ERR_VALUE        = 32
    RELAXNG_ERR_LIST         = 33
    RELAXNG_ERR_NOGRAMMAR    = 34
    RELAXNG_ERR_EXTRADATA    = 35
    RELAXNG_ERR_LACKDATA     = 36
    RELAXNG_ERR_INTERNAL     = 37
    RELAXNG_ERR_ELEMWRONG    = 38
    RELAXNG_ERR_TEXTWRONG    = 39
    # fmt: on
