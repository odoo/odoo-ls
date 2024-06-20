from typing import overload

from .._types import (
    _ET,
    SupportsLaxedItems,
    _AnyStr,
    _DefEtreeParsers,
    _ElementFactory,
    _ET_co,
    _FileReadSource,
    _NSMapArg,
    _TagName,
)
from ..html import HtmlElement
from ..objectify import ObjectifiedElement, StringElement
from ._element import _Comment, _ElementTree, _Entity, _ProcessingInstruction

def Comment(text: _AnyStr | None = None) -> _Comment: ...
def ProcessingInstruction(
    target: _AnyStr, text: _AnyStr | None = None
) -> _ProcessingInstruction: ...

PI = ProcessingInstruction

def Entity(name: _AnyStr) -> _Entity: ...

Element: _ElementFactory

# SubElement is a bit more complex than expected, as it
# handles other kinds of element, like HtmlElement
# and ObjectiedElement.
#
# - If parent is HtmlElement, generated subelement is
# HtmlElement or its relatives, depending on the tag name
# used. For example, with "label" as tag, it generates
# a LabelElement.
#
# - For ObjectifiedElement, subelements generated this way
# are always of type StringElement. Once the object is
# constructed, the object type won't change, even when
# type annotation attribute is modified.
# OE users need to use E-factory for more flexibility.
@overload
def SubElement(  # type: ignore[overload-overlap]
    _parent: ObjectifiedElement,
    _tag: _TagName,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    **_extra: _AnyStr,
) -> StringElement: ...
@overload
def SubElement(
    _parent: HtmlElement,
    _tag: _TagName,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    **_extra: _AnyStr,
) -> HtmlElement: ...
@overload
def SubElement(
    _parent: _ET,
    _tag: _TagName,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    **_extra: _AnyStr,
) -> _ET: ...
@overload  # from element, parser ignored
def ElementTree(element: _ET) -> _ElementTree[_ET]: ...
@overload  # from file source, custom parser
def ElementTree(
    element: None = None,
    *,
    file: _FileReadSource,
    parser: _DefEtreeParsers[_ET_co],
) -> _ElementTree[_ET_co]: ...
@overload  # from file source, default parser
def ElementTree(
    element: None = None,
    *,
    file: _FileReadSource,
    parser: None = None,
) -> _ElementTree: ...
