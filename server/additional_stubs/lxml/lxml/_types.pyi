import sys
from os import PathLike
from typing import (
    Any,
    Callable,
    Collection,
    Generic,
    Iterable,
    Literal,
    Mapping,
    Protocol,
    TypeVar,
)

from _typeshed import SupportsRead, SupportsWrite

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias

from .etree import HTMLParser, QName, XMLParser, _Element, _ElementTree

_KT_co = TypeVar("_KT_co", covariant=True)
_VT_co = TypeVar("_VT_co", covariant=True)

# Dup but deviate from recent _typeshed
Unused: TypeAlias = Any

# ElementTree API is notable of canonicalizing byte / unicode input data.
# This type alias should only be used for input arguments, while one would
# expect plain str in return type for most part of API (except a few places),
# as far as python3 annotation is concerned.
# Not to be confused with typing.AnyStr which is TypeVar.
_AnyStr: TypeAlias = str | bytes

# String argument also support QName in various places
_TextArg: TypeAlias = str | bytes | QName

# On the other hand, Elementpath API doesn't do str/byte canonicalization,
# only unicode accepted for py3
_ElemPathArg: TypeAlias = str | QName

# Aliases semantically indicating the purpose of text argument
_TagName: TypeAlias = _TextArg
_AttrName: TypeAlias = _TextArg
_AttrVal: TypeAlias = _TextArg

# Due to Mapping having invariant key types, Mapping[A | B, ...]
# would fail to validate against either Mapping[A, ...] or Mapping[B, ...]
# Try to settle for simpler solution, assuming python3 users would not
# use byte string as namespace prefix.
_NSMapArg = (
    Mapping[      None, _AnyStr] |
    Mapping[str       , _AnyStr] |
    Mapping[str | None, _AnyStr]
)  # fmt: skip
_NonDefaultNSMapArg = Mapping[str, _AnyStr]

# Some namespace map arguments also accept tuple form
# such as in dict()
_NSTuples: TypeAlias = Iterable[tuple[_AnyStr | None, _AnyStr]]

# Namespace mapping type specifically for Elementpath methods
#
# Actually, elementpath methods do not sanitize nsmap
# at all. It is possible to use invalid nsmap like
# {"foo": 0} and find*() method family happily accept it,
# just that they would silently fail to output any element
# afterwards. In order to be useful, both dict key and val
# must be str.
_StrictNSMap = Mapping[str, str]

# https://lxml.de/extensions.html#xpath-extension-functions
# The returned result of extension function itself is not exactly Any,
# but too complex to list.
# And xpath extension func really checks for dict in implementation,
# not just any mapping.
_XPathExtFuncArg = (
    Iterable[
        SupportsLaxedItems[
            tuple[str | None, str],
            Callable[..., Any],
        ]
    ]
    | dict[tuple[str       , str], Callable[..., Any]]
    | dict[tuple[      None, str], Callable[..., Any]]
    | dict[tuple[str | None, str], Callable[..., Any]]
)  # fmt: skip

# XPathObject documented in https://lxml.de/xpathxslt.html#xpath-return-values
# However the type is too versatile to be of any use in further processing,
# so users are encouraged to do type narrowing by themselves.
_XPathObject = Any

# XPath variable supports most of the XPathObject types
# as _input_ argument value, but most users would probably
# only use primivite types for substitution.
_XPathVarArg = (
    bool
    | int
    | float
    | str
    | bytes
    | _Element
    | list[_Element]
)  # fmt: skip

# https://lxml.de/element_classes.html#custom-element-class-lookup
_ElemClsLookupArg = Literal["element", "comment", "PI", "entity"]

# serializer.pxi _findOutputMethod()
_OutputMethodArg = Literal[
    "html",
    "text",
    "xml",
]

# saxparser.pxi _buildParseEventFilter()
_SaxEventNames = Literal[
    "start",
    "end",
    "start-ns",
    "end-ns",
    "comment",
    "pi",
]

_ET = TypeVar("_ET", bound=_Element, default=_Element)
_ET_co = TypeVar("_ET_co", bound=_Element, default=_Element, covariant=True)

class _ElementFactory(Protocol, Generic[_ET_co]):
    """Element factory protocol

    This is callback protocol for `makeelement()` method of
    various element objects, with following signature (which
    is identical to `etree.Element()` function):

    ```python
    (_tag, attrib=..., nsmap=..., **_extra)
    ```

    The mapping in `attrib` argument and all `_extra` keyword
    arguments would be merged together. The result is usually
    `{**attrib, **_extra}`, but some places may deviate.
    """

    def __call__(
        self,
        _tag: _TagName,
        /,
        attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
        nsmap: _NSMapArg | None = None,
        **_extra: _AnyStr,
    ) -> _ET_co: ...

# Note that _TagSelector filters element type not by classes,
# but checks for exact element *factory functions* instead
# (etree.Element() and friends). Python typing system doesn't
# support such outlandish usage. Use a generic callable instead.
_TagSelector: TypeAlias = _TagName | Callable[..., _Element]

_ElementOrTree: TypeAlias = _ET | _ElementTree[_ET]

# The basic parsers bundled in lxml.etree
_DefEtreeParsers = XMLParser[_ET_co] | HTMLParser[_ET_co]

class SupportsLaxedItems(Protocol[_KT_co, _VT_co]):
    """Relaxed form of SupportsItems

    Original SupportsItems from typeshed returns generic set which
    is compatible with ItemsView. However, _Attrib doesn't conform
    and returns list instead. Gotta find a common ground here.
    """

    def items(self) -> Collection[tuple[_KT_co, _VT_co]]: ...

_FilePath = _AnyStr | PathLike[str] | PathLike[bytes]
# _parseDocument() from parser.pxi
_FileReadSource = (
    _FilePath
    | SupportsRead[str]
    | SupportsRead[bytes]
)  # fmt: skip
_FileWriteSource = _FilePath | SupportsWrite[bytes]
