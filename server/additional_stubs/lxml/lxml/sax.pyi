from typing import Generic, overload
from xml.sax.handler import ContentHandler

from ._types import _ET, SupportsLaxedItems, Unused, _ElementFactory, _ElementOrTree
from .etree import LxmlError, _ElementTree, _ProcessingInstruction

class SaxError(LxmlError): ...

# xml.sax.handler is annotated in typeshed since Sept 2023.
class ElementTreeContentHandler(Generic[_ET], ContentHandler):
    _root: _ET | None
    _root_siblings: list[_ProcessingInstruction]
    _element_stack: list[_ET]
    _default_ns: str | None
    _ns_mapping: dict[str | None, list[str | None]]
    _new_mappings: dict[str | None, str]
    # Not adding _get_etree(), already available as public property
    @overload
    def __new__(
        cls, makeelement: _ElementFactory[_ET]
    ) -> ElementTreeContentHandler[_ET]: ...
    @overload
    def __new__(cls, makeelement: None = None) -> ElementTreeContentHandler: ...
    @property
    def etree(self) -> _ElementTree[_ET]: ...

    # Incompatible method overrides; some args are similar
    # but use other structures or names
    def startElementNS(  # pyright: ignore[reportIncompatibleMethodOverride]
        self,
        ns_name: tuple[str, str],
        qname: Unused,
        attributes: SupportsLaxedItems[tuple[str | None, str], str] | None = None,
    ) -> None: ...
    def endElementNS(  # pyright: ignore[reportIncompatibleMethodOverride]
        self,
        ns_name: tuple[str, str],
        qname: Unused,
    ) -> None: ...
    def characters(  # pyright: ignore[reportIncompatibleMethodOverride]
        self,
        data: str,
    ) -> None: ...
    def startElement(  # pyright: ignore[reportIncompatibleMethodOverride]
        self,
        name: str,
        attributes: SupportsLaxedItems[str, str] | None = None,
    ) -> None: ...
    ignorableWhitespace = characters  # type: ignore[assignment]

class ElementTreeProducer(Generic[_ET]):
    _element: _ET
    _content_handler: ContentHandler
    # The purpose of _attr_class and _empty_attributes is
    # more like a shortcut. These attributes are constant and
    # doesn't help debugging
    def __init__(
        self,
        element_or_tree: _ElementOrTree[_ET],
        content_handler: ContentHandler,
    ) -> None: ...
    def saxify(self) -> None: ...

# equivalent to saxify() in ElementTreeProducer
def saxify(
    element_or_tree: _ElementOrTree,
    content_handler: ContentHandler,
) -> None: ...
