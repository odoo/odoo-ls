import sys
from typing import Collection, Generic, Iterator, overload

if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self

from .._types import _ET, _AnyStr, _DefEtreeParsers, _FileReadSource
from ._element import _Element, _ElementTree

# arguments for these module funcs are the same as XML() and parse()

@overload
def XMLID(
    text: _AnyStr,
    parser: _DefEtreeParsers[_ET],
    *,
    base_url: _AnyStr | None = None,
) -> tuple[_ET, dict[str, _ET]]: ...
@overload
def XMLID(
    text: _AnyStr,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> tuple[_Element, dict[str, _Element]]: ...

# It is interesting how _IDDict is used below but not above

@overload
def XMLDTDID(
    text: _AnyStr,
    parser: _DefEtreeParsers[_ET],
    *,
    base_url: _AnyStr | None = None,
) -> tuple[_ET, _IDDict[_ET]]: ...
@overload
def XMLDTDID(
    text: _AnyStr,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> tuple[_Element, _IDDict]: ...
@overload
def parseid(
    source: _FileReadSource,
    parser: _DefEtreeParsers[_ET],
    *,
    base_url: _AnyStr | None = None,
) -> tuple[_ElementTree[_ET], _IDDict[_ET]]: ...
@overload
def parseid(
    source: _FileReadSource,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> tuple[_ElementTree, _IDDict]: ...

class _IDDict(Collection[str], Generic[_ET]):
    """Dictionary-like proxy class that mapps ID attributes to elements

    Original Docstring
    ------------------
    The dictionary must be instantiated with the root element of a parsed XML
    document, otherwise the behaviour is undefined.  Elements and XML trees
    that were created or modified 'by hand' are not supported.
    """

    def __contains__(self, __o: object) -> bool: ...
    def __getitem__(self, __k: _AnyStr) -> _ET: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    def copy(self) -> Self: ...
    def get(self, id_name: _AnyStr) -> _ET: ...
    def has_key(self, id_name: object) -> bool: ...
    def keys(self) -> list[str]: ...
    def iterkeys(self) -> Self: ...  # WTF??? Must be nobody use this.
    def items(self) -> list[tuple[str, _ET]]: ...
    def iteritems(self) -> Iterator[tuple[str, _ET]]: ...
    def values(self) -> list[_ET]: ...
    def itervalues(self) -> Iterator[_ET]: ...
