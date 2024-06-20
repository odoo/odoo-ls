#
# Note that this interface only generates lxml.etree Elements, not lxml.html ones
# See https://github.com/html5lib/html5lib-python/issues/102
#

from typing import Literal, overload

import html5lib as _html5lib
from _typeshed import SupportsRead

from .._types import _AnyStr
from ..etree import _Element, _ElementTree

# Note that tree arg is dropped, because the sole purpose of using
# this parser is to generate lxml element tree with html5lib parser.
# Other arguments good for html5lib >= 1.0
class HTMLParser(_html5lib.HTMLParser):
    def __init__(
        self,
        strict: bool = False,
        namespaceHTMLElements: bool = True,
        debug: bool = False,
    ) -> None: ...

html_parser: HTMLParser

# Notes:
# - No XHTMLParser here. Lxml tries to probe for some hypothetical
#   XHTMLParser class in html5lib which had never existed.
#   The core probing logic of this user-contributed submodule has never
#   changed since last modification at 2010. Probably yet another
#   member of code wasteland.
# - Exception raised when html=<str> and guess_charset=True
#   are used together. This is due to flawed argument passing
#   into html5lib. We cover it up with @overload's
# - Although other types of parser _might_ be usable (after implementing
#   parse() method, that is), such usage completely defeats the purpose of
#   creating this submodule. It is intended for subclassing or
#   init argument tweaking instead.

@overload
def document_fromstring(
    html: bytes,
    guess_charset: bool | None = None,
    parser: HTMLParser | None = None,
) -> _Element: ...
@overload
def document_fromstring(
    html: str,
    guess_charset: None = None,
    parser: HTMLParser | None = None,
) -> _Element: ...

# 4 overloads for fragments_fromstring:
# 2 for html (bytes/str)
# 2 for no_leading_text (true/false)
@overload  # html=bytes, no_leading_text=true
def fragments_fromstring(  # type: ignore[overload-overlap]
    html: bytes,
    no_leading_text: Literal[True],
    guess_charset: bool | None = None,
    parser: HTMLParser | None = None,
) -> list[_Element]: ...
@overload  # html=str, no_leading_text=true
def fragments_fromstring(  # type: ignore[overload-overlap]
    html: str,
    no_leading_text: Literal[True],
    guess_charset: None = None,
    parser: HTMLParser | None = None,
) -> list[_Element]: ...
@overload  # html=bytes, no_leading_text=all cases
def fragments_fromstring(
    html: bytes,
    no_leading_text: bool = False,
    guess_charset: bool | None = None,
    parser: HTMLParser | None = None,
) -> list[str | _Element]: ...
@overload  # html=str, no_leading_text=all cases
def fragments_fromstring(
    html: str,
    no_leading_text: bool = False,
    guess_charset: None = None,
    parser: HTMLParser | None = None,
) -> list[str | _Element]: ...
@overload
def fragment_fromstring(
    html: str,
    create_parent: bool | _AnyStr = False,
    guess_charset: None = None,
    parser: HTMLParser | None = None,
) -> _Element: ...
@overload
def fragment_fromstring(
    html: bytes,
    create_parent: bool | _AnyStr = False,
    guess_charset: bool | None = None,
    parser: HTMLParser | None = None,
) -> _Element: ...
@overload
def fromstring(
    html: str,
    guess_charset: None = None,
    parser: HTMLParser | None = None,
) -> _Element: ...
@overload
def fromstring(
    html: bytes,
    guess_charset: bool | None = None,
    parser: HTMLParser | None = None,
) -> _Element: ...

# html5lib doesn't support pathlib
@overload
def parse(
    filename_url_or_file: str | SupportsRead[str],
    guess_charset: None = None,
    parser: HTMLParser | None = None,
) -> _ElementTree: ...
@overload
def parse(
    filename_url_or_file: bytes | SupportsRead[bytes],
    guess_charset: bool | None = None,
    parser: HTMLParser | None = None,
) -> _ElementTree: ...
