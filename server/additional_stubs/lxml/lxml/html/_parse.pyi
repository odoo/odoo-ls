import sys
from typing import Any, Iterable, Literal, MutableMapping, overload

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias

from .. import etree
from .._types import (
    Unused,
    _AnyStr,
    _DefEtreeParsers,
    _ElemClsLookupArg,
    _FileReadSource,
)
from ._element import HtmlElement

_HtmlElemParser: TypeAlias = _DefEtreeParsers[HtmlElement]

#
# Parser
#

# Stub version before March 2023 used to omit 'target' parameter, which
# would nullify default HTML element lookup behavior, degenerating html
# submodule parsers into etree ones. Since it is decided to not support
# custom target parser for now, we just use superclass constructor for
# coherence. Same for XHTMLParser below.
class HTMLParser(etree.HTMLParser[HtmlElement]):
    """An HTML parser configured to return ``lxml.html`` Element
    objects.

    Notes
    -----
    This subclass is not specialized, unlike the ``etree`` counterpart.
    They are designed to always handle ``HtmlElement``;
    for generating other kinds of ``_Elements``, one should use
    etree parsers with ``set_element_class_lookup()`` method instead.
    In that case, see ``_FeedParser.set_element_class_lookup()`` for more info.
    """

    @property
    def target(self) -> None: ...

class XHTMLParser(etree.XMLParser[HtmlElement]):
    """An XML parser configured to return ``lxml.html`` Element
    objects.

    Annotation
    ----------
    This subclass is not specialized, unlike the ``etree`` counterpart.
    They are designed to always handle ``HtmlElement``;
    for generating other kinds of ``_Elements``, one should use
    etree parsers with ``set_element_class_lookup()`` method instead.
    In that case, see ``_FeedParser.set_element_class_lookup()`` for more info.

    Original doc
    ------------
    Note that this parser is not really XHTML aware unless you let it
    load a DTD that declares the HTML entities.  To do this, make sure
    you have the XHTML DTDs installed in your catalogs, and create the
    parser like this::

        >>> parser = XHTMLParser(load_dtd=True)

    If you additionally want to validate the document, use this::

        >>> parser = XHTMLParser(dtd_validation=True)

    For catalog support, see http://www.xmlsoft.org/catalog.html.
    """

    @property
    def target(self) -> None: ...

html_parser: HTMLParser
xhtml_parser: XHTMLParser

#
# Parsing funcs
#

# Calls etree.fromstring(html, parser, **kw) which has signature
# fromstring(text, parser, *, base_url)
def document_fromstring(
    html: _AnyStr,
    parser: _HtmlElemParser | None = None,
    ensure_head_body: bool = False,
    *,
    base_url: str | None = None,
) -> HtmlElement: ...
@overload
def fragments_fromstring(  # type: ignore[overload-overlap]
    html: _AnyStr,
    no_leading_text: Literal[True],
    base_url: str | None = None,
    parser: _HtmlElemParser | None = None,
) -> list[HtmlElement]: ...
@overload
def fragments_fromstring(
    html: _AnyStr,
    no_leading_text: bool = False,
    base_url: str | None = None,
    parser: _HtmlElemParser | None = None,
) -> list[str | HtmlElement]: ...
def fragment_fromstring(
    html: _AnyStr,
    create_parent: bool = False,
    base_url: str | None = None,
    parser: _HtmlElemParser | None = None,
) -> HtmlElement: ...
def fromstring(
    html: _AnyStr,
    base_url: str | None = None,
    parser: _HtmlElemParser | None = None,
) -> HtmlElement: ...
def parse(
    filename_or_url: _FileReadSource,
    parser: _HtmlElemParser | None = None,
    base_url: str | None = None,
) -> etree._ElementTree[HtmlElement]: ...

#
# Element Lookup
#

class HtmlElementClassLookup(etree.CustomElementClassLookup):
    def __init__(
        self,
        # Should have been something like Mapping[str, type[HtmlElement]],
        # but unfortunately classes mapping is required to be mutable
        classes: MutableMapping[str, Any] | None = None,
        # docstring says mixins is mapping, but implementation says otherwise
        mixins: Iterable[tuple[str, type[HtmlElement]]] | None = None,
    ) -> None: ...
    def lookup(  # pyright: ignore[reportIncompatibleMethodOverride]
        self,
        node_type: _ElemClsLookupArg | None,
        document: Unused,
        namespace: Unused,
        name: str,  # type: ignore[override]
    ) -> type[HtmlElement] | None: ...
