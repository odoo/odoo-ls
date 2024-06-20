#
# Parsing and other module level funcs
#

from typing import Iterable, Literal, TypeVar, overload

from .. import etree
from .._types import _AnyStr, _DefEtreeParsers, _FileReadSource
from ._element import ObjectifiedDataElement, ObjectifiedElement

_T = TypeVar("_T")

#
# Dumping tree and class lookup
#

def enable_recursive_str(on: bool) -> None:
    """Enable a recursively generated tree representation for
    `str(element)`, based on `objectify.dump(element)`"""

def dump(element: ObjectifiedElement) -> str:
    """Return a recursively generated string representation of an element"""

class ObjectifyElementClassLookup(etree.ElementClassLookup):
    """Element class lookup method that uses the objectify classes"""

    def __init__(
        self,
        tree_class: type[ObjectifiedElement] | None = None,
        empty_data_class: type[ObjectifiedDataElement] | None = None,
    ) -> None:
        """
        Parameters
        ----------
        tree_class : `type[ObjectifiedElement]`, optional
            Defines inner tree classes; it can be replaced by subclass of
            `ObjectifiedElement`. Default is None, which implies `ObjectifiedElement`.
        empty_data_class : `type[ObjectifiedDataElement]`, optional
            Defines the default class for empty data elements. Any existing
            or custom `ObjectifiedDataElement` subclass can be used.
            Default is `None`, which implies `StringElement`.
        """

#
# Parser and parsing
#

def set_default_parser(
    # Not joking, it uses isinstance check
    new_parser: etree.XMLParser[ObjectifiedElement] | None = None,
) -> None:
    """Replace the default parser used by objectify's `Element()`
    and `fromstring()` functions.

    Parameters
    ----------
    new_parser: `etree.XMLParser`, optional
        The new parser intended to replace the default one. If not
        specified, defaults to `None`, which means reverting to
        original parser.
    """

# All XMLParser() arguments, except that remove_black_text
# default value is True
def makeparser(
    *,
    encoding: _AnyStr | None = None,
    attribute_defaults: bool = False,
    dtd_validation: bool = False,
    load_dtd: bool = False,
    no_network: bool = True,
    ns_clean: bool = False,
    recover: bool = False,
    schema: etree.XMLSchema | None = None,
    huge_tree: bool = False,
    remove_blank_text: bool = True,
    resolve_entities: bool | Literal["internal"] = "internal",
    remove_comments: bool = False,
    remove_pis: bool = False,
    strip_cdata: bool = True,
    collect_ids: bool = True,
    compact: bool = True,
) -> etree.XMLParser[ObjectifiedElement]:
    """Create a new XML parser for objectify trees.

    Original Docstring
    ------------------
    You can pass all keyword arguments that are supported by
    `etree.XMLParser()`.  Note that this parser defaults to
    removing blank text.  You can disable this by passing the
    `remove_blank_text` boolean keyword option yourself.
    """

def parse(
    source: _FileReadSource,
    parser: _DefEtreeParsers[ObjectifiedElement] | None = None,
    *,
    base_url: _AnyStr | None = None,
) -> etree._ElementTree[ObjectifiedElement]:
    """Parse a file or file-like object with objectify parser

    Parameters
    ----------
    parser: `etree.XMLParser` or `etree.HTMLParser`, optional
        Using different parser is allowed. If not specified, default
        value is `None`, which means using `objectify` module's internal
        default parser.
    base_url: str or bytes, optional
        Allows setting a URL for the document when parsing from a file-like
        object. This is needed when looking up external entities
        (DTD, XInclude, ...) with relative paths.
    """

def fromstring(
    xml: _AnyStr,
    parser: _DefEtreeParsers[ObjectifiedElement] | None = None,
    *,
    base_url: _AnyStr | None = None,
) -> ObjectifiedElement:
    """Variant of corresponding `lxml.etree` function that uses objectify parser

    Parameters
    ----------
    parser: `etree.XMLParser` or `etree.HTMLParser`, optional
        Using different parser is allowed. If not specified, default
        value is `None`, which means using `objectify` module's internal
        default parser.
    base_url: str or bytes, optional
        Allows setting a URL for the document when parsing from a file-like
        object. This is needed when looking up external entities
        (DTD, XInclude, ...) with relative paths.
    """

XML = fromstring

# Not using ._types._ET, which supports PEP 696, but causes
# problem in 2nd overload of ObjectPath.__call__()
# if _ET has a default type, then all subsequent argument
# typevars need default type too (namely, _default: _T)
_ET = TypeVar("_ET", bound=etree._Element)

#
# ObjectPath -- only used within lxml.objectify
# lxml's own invention that behaves somewhat like Element Path
# https://lxml.de/objectify.html#objectpath
#
class ObjectPath:
    """`objectify`'s own path language

    This path language is modelled similar to lxml's `ETXPath`,
    but with object-like notation. Instances of this class represent
    a compiled object path.

    Example
    -------
    `root.child[1].{other}child[25]`

    See Also
    --------
    - [Web documentation](https://lxml.de/objectify.html#objectpath)
    """

    def __init__(self, path: str | Iterable[str]) -> None: ...
    @overload
    def __call__(self, root: _ET) -> _ET: ...
    @overload
    def __call__(self, root: _ET, _default: _T) -> _ET | _T: ...
    find = __call__
    def hasattr(self, root: etree._Element) -> bool: ...
    def setattr(self, root: etree._Element, value: object) -> None: ...
    def addattr(self, root: etree._Element, value: object) -> None: ...
