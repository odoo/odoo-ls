import sys
from typing import (
    Any,
    Callable,
    Iterable,
    Iterator,
    MutableMapping,
    TypeVar,
    final,
    overload,
)

if sys.version_info >= (3, 10):
    from typing import ParamSpec
else:
    from typing_extensions import ParamSpec

from .._types import SupportsLaxedItems
from ._classlookup import ElementBase, ElementClassLookup, FallbackElementClassLookup
from ._module_misc import LxmlError

_T = TypeVar("_T")
_KT = TypeVar("_KT")
_VT = TypeVar("_VT")
_P = ParamSpec("_P")
_Public_ET = TypeVar("_Public_ET", bound=ElementBase)

class LxmlRegistryError(LxmlError):
    """Base class of lxml registry errors"""

class NamespaceRegistryError(LxmlRegistryError):
    """Error registering a namespace extension"""

# Yet another dict-wannabe that lacks many normal methods and add a few
# of its own
class _NamespaceRegistry(MutableMapping[_KT, _VT]):
    def __delitem__(self, __key: _KT) -> None: ...
    def __getitem__(self, __key: _KT) -> _VT: ...
    def __setitem__(self, __key: _KT, __value: _VT) -> None: ...
    def __iter__(self) -> Iterator[_KT]: ...
    def __len__(self) -> int: ...
    def update(  # type: ignore[override]
        self,
        class_dict_iterable: SupportsLaxedItems[_KT, _VT] | Iterable[tuple[_KT, _VT]],
    ) -> None: ...
    def items(self) -> list[tuple[_KT, _VT]]: ...  # type: ignore[override]
    def iteritems(self) -> Iterator[tuple[_KT, _VT]]: ...
    def clear(self) -> None: ...

#
# Element namespace
#

@final
class _ClassNamespaceRegistry(_NamespaceRegistry[str | None, type[ElementBase]]):
    @overload  # @ns(None), @ns('tag')
    def __call__(
        self, __tag: str | None, __cls: None = None
    ) -> Callable[[type[_Public_ET]], type[_Public_ET]]: ...
    @overload  # plain @ns
    def __call__(self, __cls: type[_Public_ET]) -> type[_Public_ET]: ...

class ElementNamespaceClassLookup(FallbackElementClassLookup):
    """Element class lookup scheme that searches the Element class in the
    Namespace registry

    Example
    -------
    ```python
    lookup = ElementNamespaceClassLookup()
    ns_elements = lookup.get_namespace("http://schema.org/Movie")

    @ns_elements
    class movie(ElementBase):
      "Element implementation for 'movie' tag (using class name) in schema namespace."

    @ns_elements("movie")
    class MovieElement(ElementBase):
      "Element implementation for 'movie' tag (explicit tag name) in schema namespace."
    """

    def __init__(
        self,
        fallback: ElementClassLookup | None = None,
    ) -> None: ...
    def get_namespace(self, ns_uri: str | None) -> _ClassNamespaceRegistry:
        """Retrieve the namespace object associated with the given URI

        Pass None for the empty namespace.
        Creates a new namespace object if it does not yet exist.
        """

#
# Function namespace
#

@final
class _FunctionNamespaceRegistry(_NamespaceRegistry[str, Callable[..., Any]]):
    @property
    def prefix(self) -> str: ...
    @prefix.setter
    def prefix(self, __v: str | None) -> None: ...
    @overload  # @ns('name')
    def __call__(
        self, __name: str, __func: None = None
    ) -> Callable[[Callable[_P, _T]], Callable[_P, _T]]: ...
    @overload  # plain @ns
    def __call__(self, __func: Callable[_P, _T]) -> Callable[_P, _T]: ...

def FunctionNamespace(ns_uri: str | None) -> _FunctionNamespaceRegistry: ...
