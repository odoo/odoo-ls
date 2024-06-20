#
# ObjectifiedElement hierarchy
#

import abc
import sys
from typing import Any, Callable, Iterable, Iterator, Literal, overload

from typing_extensions import SupportsIndex

if sys.version_info >= (3, 11):
    from typing import LiteralString, Self
else:
    from typing_extensions import LiteralString, Self

from .._types import _AnyStr, _TagName
from ..cssselect import _CSSTransArg
from ..etree import CDATA, ElementBase

class ObjectifiedElement(ElementBase):
    """Main XML Element class

    Original Docstring
    ------------------
    Element children are accessed as object attributes.  Multiple children
    with the same name are available through a list index.

    Note that you cannot (and must not) instantiate this class or its
    subclasses.

    Example
    -------

    ```pycon
    >>> root = XML("<root><c1><c2>0</c2><c2>1</c2></c1></root>")
    >>> second_c2 = root.c1.c2[1]
    >>> print(second_c2.text)
    1
    ```
    """

    # Readonly, unlike _Element counterpart
    @property  # type: ignore[misc]
    def text(  # pyright: ignore[reportIncompatibleMethodOverride]
        self,
    ) -> str | None: ...
    # addattr() value is stringified before adding to attribute
    def addattr(self, tag: _TagName, value: object) -> None: ...
    def countchildren(self) -> int: ...
    def descendantpaths(self, prefix: str | list[str] | None = None) -> list[str]: ...
    def getchildren(self) -> list[ObjectifiedElement]: ...
    def __iter__(self) -> Iterator[ObjectifiedElement]: ...
    def __reversed__(self) -> Iterator[ObjectifiedElement]: ...
    def __getattr__(self, __name: str) -> ObjectifiedElement: ...
    # Input data or list need not be DataElements. They are internally
    # converted to DataElement on-the-fly. Same for __setitem__ below.
    def __setattr__(self, __name: str, __value: object) -> None: ...
    def __delattr__(self, __name: str) -> None: ...
    @overload
    def __getitem__(self, __k: int) -> ObjectifiedElement: ...
    @overload
    def __getitem__(self, __k: slice) -> list[ObjectifiedElement]: ...
    @overload
    def __setitem__(self, __k: int, __v: object) -> None: ...
    @overload
    def __setitem__(self, __k: slice, __v: Iterable[object]) -> None: ...
    def __delitem__(self, __k: int | slice) -> None: ...
    # TODO Check if _Element methods need overriding
    # CSS selector is not a normal use case for objectified
    # element (and unnecessary), but still usable nontheless
    def cssselect(
        self,
        expr: str,
        *,
        translator: _CSSTransArg = "xml",
    ) -> list[ObjectifiedElement]: ...

class ObjectifiedDataElement(ObjectifiedElement):
    """The base class for all data type Elements

    Original Docstring
    ------------------
    Subclasses should override the `pyval` property and possibly
    the `__str__` method.
    """

    # In source code, .pyval return value is stated as str. However,
    # presence of the attribute is supposed to be protocol requirement
    # for subclasses, not that people are allowed to create
    # ObjectifiedDataElement themselves which return string value for .pyval .
    @property
    @abc.abstractmethod
    def pyval(self) -> Any: ...
    def _setText(self, s: _AnyStr | CDATA | None) -> None:
        """Modify text content of objectified element directly.

        Original Docstring
        ------------------
        For use in subclasses only. Don't use unless you know what you are
        doing.
        """
    def _setValueParser(self, function: Callable[[Any], Any]) -> None:
        """Set the function that parses the Python value from a string

        Annotation notice
        -----------------
        This func originates from an abstract subclass of data element
        called `NumberElement`. Since there is no intention to construct
        such class in type annotation (yet?), the function is placed here.

        Original Docstring
        ------------------
        Do not use this unless you know what you are doing.
        """

# Forget about LongElement, which is only for Python 2.x.
#
# These data elements emulate native python data type operations,
# but lack all non-dunder methods. Too lazy to write all dunders
# one by one; directly inheriting from int and float is much more
# succint. Some day, maybe. (See known bug in BoolElement)
#
# Not doing the same for StringElement and BoolElement though,
# each for different reason.
class IntElement(ObjectifiedDataElement, int):
    @property
    def pyval(self) -> int: ...
    @property  # type: ignore[misc]
    def text(self) -> str: ...  # type: ignore[override]

class FloatElement(ObjectifiedDataElement, float):
    @property
    def pyval(self) -> float: ...
    @property  # type: ignore[misc]
    def text(self) -> str: ...  # type: ignore[override]

class StringElement(ObjectifiedDataElement):
    """String data class

    Note that this class does *not* support the sequence protocol of strings:
    `len()`, `iter()`, `str_attr[0]`, `str_attr[0:1]`, etc. are *not* supported.
    Instead, use the `.text` attribute to get a 'real' string.
    """

    # For empty string element, .pyval = __str__ = '', .text = None
    @property
    def pyval(self) -> str: ...
    def strlen(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __ge__(self, other: Self | str) -> bool: ...
    def __gt__(self, other: Self | str) -> bool: ...
    def __le__(self, other: Self | str) -> bool: ...
    def __lt__(self, other: Self | str) -> bool: ...
    # Stringify any object before concat
    def __add__(self, other: object) -> str: ...
    def __radd__(self, other: object) -> str: ...
    def __mul__(self, other: SupportsIndex) -> str: ...
    def __rmul__(self, other: SupportsIndex) -> str: ...
    @overload
    def __mod__(
        self, other: LiteralString | tuple[LiteralString, ...]
    ) -> LiteralString: ...
    @overload
    def __mod__(self, other: object) -> str: ...
    def __float__(self) -> float: ...
    def __int__(self) -> int: ...
    def __complex__(self) -> complex: ...

class NoneElement(ObjectifiedDataElement):
    @property
    def pyval(self) -> None: ...
    @property  # type: ignore[misc]
    def text(self) -> None: ...  # type: ignore[override]
    def __bool__(self) -> Literal[False]: ...

# BoolElement can't inherit from bool, which is marked @final
class BoolElement(IntElement):
    @property
    def pyval(self) -> bool: ...
    @property  # type: ignore[misc]
    def text(self) -> str: ...  # type: ignore[override]
    def __bool__(self) -> bool: ...
    def __int__(self) -> int: ...
    def __float__(self) -> float: ...
    # FIXME Unlike arbitrary floating point / integer powers,
    # power involving bool always have fixed results (0 or 1).
    # However, python maintainers have delved into some disgusting
    # sort of half-arsed "type annotation arithmatics":
    # - int**0  = Literal[1]
    # - int**25 = int
    # - int**26 = Any
    # - int**-20 = float
    # - int**-21 = Any
    # It isn't a wise decision spending time to construct overloads
    # matching that idiocy, so let's skip implementing __pow__ for
    # now, until BoolElement become independent.
    @overload
    def __and__(self, __n: bool | BoolElement) -> bool: ...
    @overload
    def __and__(self, __n: int) -> int: ...
    @overload
    def __or__(self, __n: bool | BoolElement) -> bool: ...
    @overload
    def __or__(self, __n: int) -> int: ...
    @overload
    def __xor__(self, __n: bool | BoolElement) -> bool: ...
    @overload
    def __xor__(self, __n: int) -> int: ...
    # FIXME Reverse boolean operators, see #13
    # To be fixed: __rpow__, __rand__, __ror__, __rxor__
