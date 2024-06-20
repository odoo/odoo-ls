import sys

if sys.version_info >= (3, 11):
    from typing import LiteralString
else:
    from typing_extensions import LiteralString

from typing import Literal, Protocol, overload

from lxml.etree import LxmlSyntaxError, _Element

from ._types import Unused, _ElementOrTree

# exported constants
XINCLUDE: LiteralString
XINCLUDE_INCLUDE: LiteralString
XINCLUDE_FALLBACK: LiteralString
XINCLUDE_ITER_TAG: LiteralString
DEFAULT_MAX_INCLUSION_DEPTH: int

class FatalIncludeError(LxmlSyntaxError): ...
class LimitedRecursiveIncludeError(FatalIncludeError): ...

# The default_loader() in lxml.ElementInclude is completely
# retired (lxml uses its own internal loader)

class LoaderProtocol(Protocol):
    """Protocol for loader func argument in `ElementInclude.include()`

    Annotation
    ----------
    `loader=` argument in `ElementInclude.include()` specifies the function
    object to load URL or file resource. It has the following overloaded
    function signature:
    - `(_href: str, _mode: Literal["xml"], /, encoding: str = None) -> _Element`
    - `(_href: str, _mode: Literal["text"], /, encoding: str = None) -> str`
    """

    @overload
    def __call__(
        self,
        _href: str,  # URL or local path from href="..." attribute
        _mode: Literal["xml"],
        /,
        encoding: Unused = None,  # Under XML mode this param is ignored
        # but must be present nontheless
    ) -> _Element: ...
    @overload
    def __call__(
        self,
        _href: str,
        _mode: Literal["text"],
        /,
        encoding: str | None = None,
    ) -> str: ...

def include(
    elem: _ElementOrTree,
    loader: LoaderProtocol | None = None,
    base_url: str | None = None,
    max_depth: int = 6,
) -> None:
    """Expand XInclude directives

    Annotation
    ----------
    - Source documentation above `include()` is outdated; this function
    does not return at all.
    - Try using `from lxml.ElementInclude import LoaderProtocol` from
    within IDEs to lookup its purpose and usage. This is annotation
    only and doesn't exist in lxml source.
    """
