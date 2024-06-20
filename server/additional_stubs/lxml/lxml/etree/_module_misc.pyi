#
# lxml.etree helper classes, exceptions and constants
#

import sys
from abc import ABCMeta, abstractmethod
from typing import overload

if sys.version_info >= (3, 11):
    from typing import LiteralString
else:
    from typing_extensions import LiteralString

from .._types import _AnyStr, _ElementOrTree, _TagName
from ._dtd import DTD
from ._element import _Element
from ._xmlerror import _BaseErrorLog, _ListErrorLog

DEBUG: int
ICONV_COMPILED_VERSION: tuple[int, int]
LIBXML_VERSION: tuple[int, int, int]
LIBXML_COMPILED_VERSION: tuple[int, int, int]
LXML_VERSION: tuple[int, int, int, int]
__version__: LiteralString

class DocInfo:
    # Can't be empty, otherwise it means tree contains no element
    @property
    def root_name(self) -> str: ...
    @property
    def public_id(self) -> str | None: ...
    @public_id.setter
    def public_id(self, __v: _AnyStr | None) -> None: ...
    @property
    def system_url(self) -> str | None: ...
    @system_url.setter
    def system_url(self, __v: _AnyStr | None) -> None: ...
    @property
    def xml_version(self) -> str: ...  # fallback is "1.0"
    @property
    def encoding(self) -> str: ...  # fallback is "UTF-8" or "ISO-8859-1"
    @property
    def standalone(self) -> bool | None: ...
    @property
    def URL(self) -> str | None: ...
    @URL.setter
    def URL(self, __v: _AnyStr | None) -> None: ...
    @property
    def doctype(self) -> str: ...
    @property
    def internalDTD(self) -> DTD | None: ...
    @property
    def externalDTD(self) -> DTD | None: ...
    def clear(self) -> None: ...

class QName:
    @overload
    def __init__(
        self,
        text_or_uri_or_element: _TagName | _Element,
        tag: _TagName | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        text_or_uri_or_element: None,
        tag: _TagName,
    ) -> None: ...
    @property
    def localname(self) -> str: ...
    @property
    def namespace(self) -> str | None: ...
    @property
    def text(self) -> str: ...
    # Emulate __richcmp__()
    def __ge__(self, other: _TagName) -> bool: ...
    def __gt__(self, other: _TagName) -> bool: ...
    def __le__(self, other: _TagName) -> bool: ...
    def __lt__(self, other: _TagName) -> bool: ...

class CDATA:
    def __init__(self, data: _AnyStr) -> None: ...

class Error(Exception): ...

class LxmlError(Error):
    def __init__(
        self, message: object, error_log: _BaseErrorLog | None = None
    ) -> None: ...
    # Even when LxmlError is initiated with PyErrorLog, it fools
    # error_log property by creating a dummy _ListErrorLog object
    error_log: _ListErrorLog

class DocumentInvalid(LxmlError): ...
class LxmlSyntaxError(LxmlError, SyntaxError): ...
class C14NError(LxmlError): ...

class _Validator(metaclass=ABCMeta):
    def assert_(self, etree: _ElementOrTree) -> None: ...
    def assertValid(self, etree: _ElementOrTree) -> None: ...
    def validate(self, etree: _ElementOrTree) -> bool: ...
    @property
    def error_log(self) -> _ListErrorLog: ...
    # all methods implicitly require a concrete __call__()
    # implementation in subclasses in order to be usable
    @abstractmethod
    def __call__(self, etree: _ElementOrTree) -> bool: ...

# Though etree.Schematron is not implemented in stub,
# lxml.isoschematron reuses related exception classes,
# so list them here
class SchematronError(LxmlError):
    """Base class of all Schematron errors"""

class SchematronParseError(SchematronError):
    """Error while parsing an XML document as Schematron schema"""

class SchematronValidateError(SchematronError):
    """Error while validating an XML document with a Schematron schema"""
