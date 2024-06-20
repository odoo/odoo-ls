import sys
from abc import ABCMeta, abstractmethod
from typing import Any, final, type_check_only

from _typeshed import SupportsRead

if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self

from .._types import _AnyStr

@type_check_only
class _InputDocument:
    """An internal opaque object used as resolver result"""

@type_check_only
class _ResolverContext:
    """An internal opaque object used in resolve methods"""

class Resolver(metaclass=ABCMeta):
    @abstractmethod
    def resolve(
        self, system_url: str, public_id: str, context: _ResolverContext
    ) -> _InputDocument | None: ...
    def resolve_empty(self, context: _ResolverContext) -> _InputDocument: ...
    def resolve_string(
        self,
        string: _AnyStr,
        context: _ResolverContext,
        *,
        base_url: _AnyStr | None = None,
    ) -> _InputDocument: ...
    def resolve_filename(
        self, filename: _AnyStr, context: _ResolverContext
    ) -> _InputDocument: ...
    def resolve_file(
        self,
        f: SupportsRead[Any],
        context: _ResolverContext,
        *,
        base_url: _AnyStr | None,
        close: bool,
    ) -> _InputDocument: ...

@final
class _ResolverRegistry:
    def add(self, resolver: Resolver) -> None: ...
    def remove(self, resolver: Resolver) -> None: ...
    def copy(self) -> Self: ...
    # Wonder if resolve() should be removed. It's not like one
    # can supply the internal context object at all. So far it
    # is only used internally.
    def resolve(
        self, system_url: str, public_id: str, context: _ResolverContext
    ) -> _InputDocument | None: ...
