from typing import Any, Callable, Generic, Mapping, Protocol, overload

from ._types import _AnyStr, _ElementFactory, _ET_co, _NSMapArg, _NSTuples, _TagName
from .etree import CDATA, _Element

# Mapping should have been something like
# Mapping[type[_T], Callable[[_Element, _T], None]]
# but invariant key/value causes it to be incompatible
# with anything
_TypeMapArg = Mapping[Any, Callable[[_Element, Any], None]]

class _EMakerCallProtocol(Protocol[_ET_co]):
    def __call__(
        self,
        # By default ElementMaker only accepts _Element and types
        # interpretable by default typemap (str, CDATA and dict).
        # Typemap can be expanded manually to accept object of
        # any type, but such usage isn't very common, so we
        # concentrate on default user case instead.
        # Extra notes:
        # - Although builder expects to be nested, its
        #   implementation allows just any function object
        #   as children
        # - Child element type need not follow parent type.
        #   This is much more apparent in e.g. HtmlElement
        *_children: _Element
        | str
        | CDATA
        | dict[str, Any]
        | Callable[[], _Element | str | CDATA | dict[str, Any]],
        **_attrib: str,
    ) -> _ET_co: ...
    # Following properties come from functools.partial
    @property
    def func(self) -> ElementMaker[_ET_co]: ...
    @property
    def args(self) -> tuple[str]: ...
    @property
    def keywords(self) -> dict[str, Any]: ...

# One might be tempted to use artibrary callable in
# makeelement argument, because ElementMaker
# constructor can actually accept any callable as
# makeelement. However all element creation attempt
# would fail, as 'nsmap' keyword argument is expected
# to be usable in the makeelement function call.
class ElementMaker(Generic[_ET_co]):
    @overload  # makeelement is keyword
    def __new__(
        cls,
        typemap: _TypeMapArg | None = None,
        namespace: str | None = None,
        nsmap: _NSMapArg | _NSTuples | None = None,  # dict()
        *,
        makeelement: _ElementFactory[_ET_co],
    ) -> ElementMaker[_ET_co]: ...
    @overload  # makeelement is positional
    def __new__(
        cls,
        typemap: _TypeMapArg | None,
        namespace: str | None,
        nsmap: _NSMapArg | _NSTuples | None,
        makeelement: _ElementFactory[_ET_co],
    ) -> ElementMaker[_ET_co]: ...
    @overload  # makeelement is default or absent
    def __new__(
        cls,
        typemap: _TypeMapArg | None = None,
        namespace: str | None = None,
        nsmap: _NSMapArg | _NSTuples | None = None,
        makeelement: None = None,
    ) -> ElementMaker: ...
    def __call__(
        self,
        tag: _TagName,
        *_children: _Element  # See _EMakerCallProtocol above
        | str
        | CDATA
        | dict[str, Any]
        | Callable[[], _Element | str | CDATA | dict[str, Any]],
        **_attrib: str,
    ) -> _ET_co: ...

    # __getattr__ here is special. ElementMaker supports using any
    # attribute name as tag, returning a functools.partial
    # object to ElementMaker.__call__() with tag argument prefilled.
    # So E('html', ...) is equivalent to E.html(...).
    # However, annotation of returning partial is vetoed,
    # as it has a very generic call signature in typeshed.
    # The confined call signature is more important for
    # users. So opt for adding partial properties to the protocol.
    def __getattr__(self, name: str) -> _EMakerCallProtocol[_ET_co]: ...

    # Private read-only attributes, could be useful for understanding
    # how the ElementMaker is constructed
    # Note that the corresponding input arguments during ElementMaker
    # instantiation are much more relaxed, as typing.Mapping argument
    # invariance has posed some challenge to typing. We can afford
    # some more restriction as return value or attribute.
    @property
    def _makeelement(self) -> _ElementFactory[_ET_co]: ...
    @property
    def _namespace(self) -> str | None: ...
    @property
    def _nsmap(self) -> dict[str | None, _AnyStr] | None: ...
    @property
    def _typemap(self) -> dict[type[Any], Callable[[_ET_co, Any], None]]: ...

E: ElementMaker
