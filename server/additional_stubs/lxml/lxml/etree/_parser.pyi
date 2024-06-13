import sys
from typing import Any, Collection, Generic, Iterable, Iterator, Literal, TypeVar

if sys.version_info >= (3, 11):
    from typing import LiteralString, Self
else:
    from typing_extensions import LiteralString, Self

if sys.version_info >= (3, 13):
    from warnings import deprecated
else:
    from typing_extensions import deprecated

from .._types import (
    _AnyStr,
    _DefEtreeParsers,
    _ElementFactory,
    _ET_co,
    _SaxEventNames,
    _TagSelector,
)
from ._classlookup import ElementClassLookup
from ._docloader import _ResolverRegistry
from ._element import _Element
from ._module_misc import LxmlError, LxmlSyntaxError
from ._saxparser import ParserTarget
from ._xmlerror import _ListErrorLog
from ._xmlschema import XMLSchema

_T = TypeVar("_T")

class ParseError(LxmlSyntaxError):
    lineno: int  # pyright: ignore[reportIncompatibleVariableOverride]
    offset: int  # pyright: ignore[reportIncompatibleVariableOverride]
    code: int
    filename: str | None
    position: tuple[int, int]
    def __init__(
        self,
        message: object,
        code: int,
        line: int,
        column: int,
        filename: str | None = None,
    ) -> None: ...

class XMLSyntaxError(ParseError): ...
class ParserError(LxmlError): ...

# Includes most stuff in _BaseParser
class _FeedParser(Generic[_ET_co]):
    @property
    def error_log(self) -> _ListErrorLog: ...
    @property
    def resolvers(self) -> _ResolverRegistry: ...
    @property
    def version(self) -> LiteralString: ...
    def copy(self) -> Self: ...
    makeelement: _ElementFactory[_ET_co]
    # In terms of annotation, what setting class_lookup does
    # is change _ET_co (type specialization), which can't be
    # done automatically with current python typing system.
    # One has to change it manually during type checking.
    # Very few people would do, if there were any at all.
    def set_element_class_lookup(
        self, lookup: ElementClassLookup | None = None
    ) -> None:
        """
        Notes
        -----
        When calling this method, it is advised to also change typing
        specialization of concerned parser too, because current python
        typing system can't change it automatically.

        Example
        -------
        Following code demonstrates how to create ``lxml.html.HTMLParser``
        manually from ``lxml.etree.HTMLParser``::

        ```python
        parser = etree.HTMLParser()
        reveal_type(parser)  # HTMLParser[_Element]
        if TYPE_CHECKING:
            parser = cast('etree.HTMLParser[HtmlElement]', parser)
        else:
            parser.set_element_class_lookup(
                html.HtmlElementClassLookup())
        result = etree.fromstring(data, parser=parser)
        reveal_type(result)  # HtmlElement
        ```
        """
        ...

    @deprecated("Removed since 5.0; renamed to set_element_class_lookup()")
    def setElementClassLookup(
        self, lookup: ElementClassLookup | None = None
    ) -> None: ...
    @property
    def feed_error_log(self) -> _ListErrorLog: ...
    def feed(self, data: _AnyStr) -> None: ...

# Custom parser target support is abandoned,
# see comment in XMLParser
class _ParserTargetMixin(Generic[_T]):
    @property
    def target(self) -> ParserTarget[_T] | None: ...
    def close(self) -> _T: ...

class _PullParserMixin:
    # The iterated items from pull parser events may return anything.
    # Even etree.TreeBuilder, which produce element nodes by default, allows
    # overriding factory functions via arguments to generate anything.
    def read_events(self) -> Iterator[tuple[str, _Element | Any]]: ...

# It is unfortunate that, in the end, it is decided to forfeit
# integration of custom target annotation (the 'target' parameter).
# So far all attempts would cause usage of annotation unnecessarily
# complex and convoluted, yet still can't get everything right.
class XMLParser(_ParserTargetMixin[Any], _FeedParser[_ET_co]):
    def __init__(
        self,
        *,
        encoding: _AnyStr | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        ns_clean: bool = False,
        recover: bool = False,
        schema: XMLSchema | None = None,
        huge_tree: bool = False,
        remove_blank_text: bool = False,
        resolve_entities: bool | Literal["internal"] = "internal",
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        collect_ids: bool = True,
        target: ParserTarget[Any] | None = None,
        compact: bool = True,
    ) -> None: ...

class XMLPullParser(_PullParserMixin, XMLParser[_ET_co]):
    def __init__(
        self,
        events: Iterable[_SaxEventNames] | None = None,
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        base_url: _AnyStr | None = None,
        # All arguments from XMLParser
        encoding: _AnyStr | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        ns_clean: bool = False,
        recover: bool = False,
        schema: XMLSchema | None = None,
        huge_tree: bool = False,
        remove_blank_text: bool = False,
        resolve_entities: bool | Literal["internal"] = "internal",
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        collect_ids: bool = True,
        target: ParserTarget[Any] | None = None,
        compact: bool = True,
    ) -> None: ...

# This is XMLParser with some preset keyword arguments, and without
# 'collect_ids' argument. Removing those keywords here, otherwise
# ETCompatXMLParser has no reason to exist.
class ETCompatXMLParser(XMLParser[_ET_co]):
    def __init__(
        self,
        *,
        encoding: _AnyStr | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        ns_clean: bool = False,
        recover: bool = False,
        schema: XMLSchema | None = None,
        huge_tree: bool = False,
        remove_blank_text: bool = False,
        resolve_entities: bool | Literal["internal"] = True,
        strip_cdata: bool = True,
        target: ParserTarget[Any] | None = None,
        compact: bool = True,
    ) -> None: ...

def set_default_parser(parser: _DefEtreeParsers[Any] | None) -> None: ...
def get_default_parser() -> _DefEtreeParsers[Any]: ...

class HTMLParser(_ParserTargetMixin[Any], _FeedParser[_ET_co]):
    def __init__(
        self,
        *,
        encoding: _AnyStr | None = None,
        remove_blank_text: bool = False,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        no_network: bool = True,
        target: ParserTarget[Any] | None = None,
        schema: XMLSchema | None = None,
        recover: bool = True,
        compact: bool = True,
        default_doctype: bool = True,
        collect_ids: bool = True,
        huge_tree: bool = False,
    ) -> None: ...

class HTMLPullParser(_PullParserMixin, HTMLParser[_ET_co]):
    def __init__(
        self,
        events: Iterable[_SaxEventNames] | None = None,
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        base_url: _AnyStr | None = None,
        # All arguments from HTMLParser
        encoding: _AnyStr | None = None,
        remove_blank_text: bool = False,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        no_network: bool = True,
        target: ParserTarget[Any] | None = None,
        schema: XMLSchema | None = None,
        recover: bool = True,
        compact: bool = True,
        default_doctype: bool = True,
        collect_ids: bool = True,
        huge_tree: bool = False,
    ) -> None: ...
