#
# Element factories
#

from typing import Any, Literal, Protocol, TypeVar, overload

from .._types import SupportsLaxedItems, _AnyStr, _ElementFactory, _NSMapArg, _TagName
from ..etree import _Element
from . import _element as _e

_DataElem_T = TypeVar("_DataElem_T", bound=_e.ObjectifiedDataElement)

def Element(
    _tag: _TagName,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: str | None = None,
    **__attr: _AnyStr,
) -> _e.ObjectifiedElement:
    """Objectify specific version of `lxml.etree` `Element()` factory

    Original Docstring
    ------------------
    Requires parser based element class lookup activated in `lxml.etree`!

    Parameters
    ----------
    _tag : str, bytes or QName
        Element tag name
    attrib : mapping of string key/value, optional
        Attributes to be added to element. Default is None.
    nsmap : mapping of namespace prefix/URI, optional
        Extra namespaces added to element. Default is None.
    _pytype : str | None, optional
        The `pytype` to be used for this element. Default is None, which implies
        no pytype annotation would be added to inner tree element. See `PyType`
        class docstring for more info.

    Returns
    -------
    ObjectifiedElement
        The generated element.
    """

def SubElement(
    _parent: _e.ObjectifiedElement,
    _tag: _TagName,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    **__attr: _AnyStr,
) -> _e.ObjectifiedElement: ...

# TODO Current overload situation is unsatisfactory. Will decide
# whether the list should be trimmed or extended in future.
#
# XXX Order matters! float can't be listed before int
#
@overload  # DataElement retains same type if no other hint given
def DataElement(
    _value: _DataElem_T,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: None = None,
    _xsi: None = None,
    **__attr: _AnyStr,
) -> _DataElem_T: ...
@overload  # native type None
def DataElement(
    _value: None,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: None = None,
    _xsi: None = None,
    **__attr: _AnyStr,
) -> _e.NoneElement: ...
@overload  # native type str
def DataElement(
    _value: str,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: None = None,
    _xsi: None = None,
    **__attr: _AnyStr,
) -> _e.StringElement: ...
@overload  # native type bool
def DataElement(  # pyright: ignore[reportOverlappingOverload]
    _value: bool,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: None = None,
    _xsi: None = None,
    **__attr: _AnyStr,
) -> _e.BoolElement: ...
@overload  # native type int
def DataElement(
    _value: int,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: None = None,
    _xsi: None = None,
    **__attr: _AnyStr,
) -> _e.IntElement: ...
@overload  # native type float
def DataElement(
    _value: float,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: None = None,
    _xsi: None = None,
    **__attr: _AnyStr,
) -> _e.FloatElement: ...
@overload  # pytype None
def DataElement(
    _value: object,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: Literal["NoneType", "none"],
    _xsi: str | None = None,
    **__attr: _AnyStr,
) -> _e.NoneElement: ...
@overload  # pytype str
def DataElement(
    _value: object,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: Literal["str"],
    _xsi: str | None = None,
    **__attr: _AnyStr,
) -> _e.StringElement: ...
@overload  # pytype bool
def DataElement(
    _value: object,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: Literal["bool"],
    _xsi: str | None = None,
    **__attr: _AnyStr,
) -> _e.BoolElement: ...
@overload  # pytype int
def DataElement(
    _value: object,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: Literal["int"],
    _xsi: str | None = None,
    **__attr: _AnyStr,
) -> _e.IntElement: ...
@overload  # pytype float
def DataElement(
    _value: object,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: Literal["float"],
    _xsi: str | None = None,
    **__attr: _AnyStr,
) -> _e.FloatElement: ...
@overload  # Generic fallback
def DataElement(
    _value: object,
    /,
    attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
    nsmap: _NSMapArg | None = None,
    *,
    _pytype: str | None = None,
    _xsi: str | None = None,
    **__attr: _AnyStr,
) -> _e.ObjectifiedElement:
    """Create a new element from a Python value and XML attributes taken
    from keyword arguments or a dictionary passed as second argument.

    Annotation notice
    -----------------
    1. Current DataElement stub overloads only represent cases where
    data type is inferred from value or from lxml's own PyType arg.
    If XMLSchema Interface types are involved, the overload list
    will become unbearably long (well, it alreaday is), which is
    both a nightmare for maintainer and users alike. As a result,
    please read description of `_xsi` parameter carefully
    before using, which is not covered in original docstring.

    2. `DataElement()` allows a special case handling: if value
    is an `ObjectifiedElement` (not data element), AND no other
    arguments are supplied, simply return a copy of itself.
    Such silly usage is ignored here, when `copy.copy(element)`
    already suffices.

    Original Docstring
    ------------------
    Automatically guesses `py:pytype` attribute from basic Python data type of
    the value if it can be identified.  If `_pytype` or `_xsi` are among the
    keyword arguments, they will be used instead of auto detection.

    If the `_value` argument is an `ObjectifiedDataElement` instance,
    its `py:pytype`, `xsi:type`, `nsmap` and other attributes are
    reused unless redefined in attrib and/or keyword arguments.

    See Also
    --------
    - [XMLSchema Instances data types](https://www.w3.org/TR/2004/REC-xmlschema-2-20041028/datatypes.html)
    - [How lxml determines data type](https://lxml.de/objectify.html#how-data-types-are-matched)

    Parameters
    ----------
    _value : Any
        The value to be used in new element
    attrib : mapping of str to str, optional
        Attributes to be added to element, by default `None`.
        Usually specified as a `dict` or lxml's own `_Attrib`.
    nsmap : mapping of str to str, optional
        Mapping of namespace prefixes to URI. Default is `None`, which uses
        lxml's internal mapping.
    _pytype : str, keyword, optional
        Coerce value into specified data type, such as `"int"` or `"bool"`.
        Default is `None`, which means data type is autodetected by other
        mechanisms.
    _xsi : str, keyword, optional
        lxml can use XMLSchema Instances data types to help guessing correct
        pytype (see previous parameter). Some of the XSI data types
        can be used, and the `nsd:` namespace prefix is optional.
        Beware that this argument becomes a dummy if `_pytype` argument
        is specified, yet still would be present in element, even if it's
        wrong or can't be resolved. Default is `None`, which means
        XSI data type is not involved in type determination.
    """

class _OEMakerCallProtocol(Protocol):
    """Callback Protocol for Objectified ElementMaker

    Annotation
    ----------
    This is the call signature of `lxml.objectify.ElementMaker`
    with `tag` argument removed.
    Thus arguments are effectively the same as, say `E.html()`,
    with all keyword arguments as tag attributes, and positional
    arguments as child element or tag content.
    """
    def __call__(
        self,
        *_children: _Element
        | _AnyStr
        | bool
        | int
        | float
        | dict[str, Any]
        | _OEMakerCallProtocol
        | None,
        **_attrib: _AnyStr,
    ) -> _e.ObjectifiedElement: ...

class ElementMaker:
    """Used for constructing trees

    Note that this module has a predefined `ElementMaker` instance called `E`.

    Example
    -------

    ```pycon
    >>> M = ElementMaker(annotate=False)
    >>> attributes = {'class': 'par'}
    >>> html = M.html( M.body( M.p('hello', attributes, M.br, 'objectify', style="font-weight: bold") ) )

    >>> from lxml.etree import tostring
    >>> print(tostring(html, method='html').decode('ascii'))
    <html><body><p style="font-weight: bold" class="par">hello<br>objectify</p></body></html>
    ```

    To create tags that are not valid Python identifiers, call the factory
    directly and pass the tag name as first argument::

    ```pycon
    >>> root = M('tricky-tag', 'some text')
    >>> print(root.tag)
    tricky-tag
    >>> print(root.text)
    some text
    ```
    """

    def __init__(
        self,
        *,
        namespace: str | None = None,
        nsmap: _NSMapArg | None = None,
        annotate: bool = True,
        makeelement: _ElementFactory[_e.ObjectifiedElement] | None = None,
    ) -> None: ...
    # Note:
    # - Attribute values supplied as children dict will be stringified,
    #   but those as keyword argument _must_ be string itself as they
    #   are not converted
    # - One single child of value 'None' is special, but that doesn't
    #   affect typing
    # - Default children accepts all builtin data types understood
    #   by ObjectifiedElement (bool, float, string etc). In addition,
    #   The PyType registry can register additional support of other
    #   object types. Yet it is decided to not accept anything here,
    #   as even object of wrong type can be used in runtime which
    #   is forcefully stringified into garbage data.
    def __call__(
        self,
        tag: str,  # bytes or namespaced QName object unsupported
        *_children: _Element  # See _OEMakerCallProtocol above
        | _AnyStr
        | bool
        | int
        | float
        | dict[str, Any]
        | _OEMakerCallProtocol
        | None,
        **_attrib: _AnyStr,
    ) -> _e.ObjectifiedElement: ...
    # __getattr__ here is special. ElementMaker supports using any
    # attribute name as tag, which is sort of like a functools.partial
    # object to ElementMaker.__call__() with tag argument prefilled.
    # So E('html', ...) is equivalent to E.html(...)
    def __getattr__(self, tag: str) -> _OEMakerCallProtocol: ...

E: ElementMaker
