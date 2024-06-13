from typing import Any, Sequence, overload

from _typeshed import SupportsRead
from bs4 import BeautifulSoup, PageElement, SoupStrainer
from bs4.builder import TreeBuilder

from .._types import _ET, _AnyStr, _ElementFactory
from ..etree import _ElementTree
from . import HtmlElement

# NOTES:
# - kw only arguments for fromstring() and parse() are
#   taken from types-beautifulsoup4
# - annotation for 'features' argument should have been
#
#       features: str | Sequence[str] | None = None
#
#   but current modification is much more helpful for users
# - makeelement argument provides very exotic feature:
#   it's actually possible to convert BeautifulSoup html tree
#   into lxml XML element tree, not just lxml html tree

@overload  # makeelement is positional
def fromstring(
    data: _AnyStr | SupportsRead[str] | SupportsRead[bytes],
    beautifulsoup: type[BeautifulSoup] | None,
    makeelement: _ElementFactory[_ET],
    *,
    features: str | Sequence[str] = "html.parser",
    builder: TreeBuilder | type[TreeBuilder] | None = None,
    parse_only: SoupStrainer | None = None,
    from_encoding: str | None = None,
    exclude_encodings: Sequence[str] | None = None,
    element_classes: dict[type[PageElement], type[Any]] | None = None,
) -> _ET: ...
@overload  # makeelement is kw
def fromstring(
    data: _AnyStr | SupportsRead[str] | SupportsRead[bytes],
    beautifulsoup: type[BeautifulSoup] | None = None,
    *,
    makeelement: _ElementFactory[_ET],
    features: str | Sequence[str] = "html.parser",
    builder: TreeBuilder | type[TreeBuilder] | None = None,
    parse_only: SoupStrainer | None = None,
    from_encoding: str | None = None,
    exclude_encodings: Sequence[str] | None = None,
    element_classes: dict[type[PageElement], type[Any]] | None = None,
) -> _ET: ...
@overload  # makeelement not provided or is default
def fromstring(
    data: _AnyStr | SupportsRead[str] | SupportsRead[bytes],
    beautifulsoup: type[BeautifulSoup] | None = None,
    makeelement: None = None,
    *,
    features: str | Sequence[str] = "html.parser",
    builder: TreeBuilder | type[TreeBuilder] | None = None,
    parse_only: SoupStrainer | None = None,
    from_encoding: str | None = None,
    exclude_encodings: Sequence[str] | None = None,
    element_classes: dict[type[PageElement], type[Any]] | None = None,
) -> HtmlElement: ...

# Technically Path is also accepted for parse() file argument
# but emits visible warning
@overload  # makeelement is positional
def parse(
    file: _AnyStr | SupportsRead[str] | SupportsRead[bytes],
    beautifulsoup: type[BeautifulSoup] | None,
    makeelement: _ElementFactory[_ET],
    *,
    features: str | Sequence[str] = "html.parser",
    builder: TreeBuilder | type[TreeBuilder] | None = None,
    parse_only: SoupStrainer | None = None,
    from_encoding: str | None = None,
    exclude_encodings: Sequence[str] | None = None,
    element_classes: dict[type[PageElement], type[Any]] | None = None,
) -> _ElementTree[_ET]: ...
@overload
def parse(  # makeelement is kw
    file: _AnyStr | SupportsRead[str] | SupportsRead[bytes],
    beautifulsoup: type[BeautifulSoup] | None = None,
    *,
    makeelement: _ElementFactory[_ET],
    features: str | Sequence[str] = "html.parser",
    builder: TreeBuilder | type[TreeBuilder] | None = None,
    parse_only: SoupStrainer | None = None,
    from_encoding: str | None = None,
    exclude_encodings: Sequence[str] | None = None,
    element_classes: dict[type[PageElement], type[Any]] | None = None,
) -> _ElementTree[_ET]: ...
@overload  # makeelement not provided or is default
def parse(
    file: _AnyStr | SupportsRead[str] | SupportsRead[bytes],
    beautifulsoup: type[BeautifulSoup] | None = None,
    makeelement: None = None,
    *,
    features: str | Sequence[str] = "html.parser",
    builder: TreeBuilder | type[TreeBuilder] | None = None,
    parse_only: SoupStrainer | None = None,
    from_encoding: str | None = None,
    exclude_encodings: Sequence[str] | None = None,
    element_classes: dict[type[PageElement], type[Any]] | None = None,
) -> _ElementTree[HtmlElement]: ...
@overload
def convert_tree(
    beautiful_soup_tree: BeautifulSoup,
    makeelement: _ElementFactory[_ET],
) -> list[_ET]: ...
@overload
def convert_tree(
    beautiful_soup_tree: BeautifulSoup,
    makeelement: None = None,
) -> list[HtmlElement]: ...
