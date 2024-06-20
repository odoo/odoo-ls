from typing import Callable, Iterable, TypeVar

from .._types import _AnyStr
from ..etree import _Element

_T = TypeVar("_T")

# annotation attribute can be anything, which is stringified
# later on; but the type would better be consistent though
def html_annotate(
    doclist: Iterable[tuple[str, _T]],
    markup: Callable[[str, _T], str] = ...,  # keep ellipsis
) -> str: ...
def htmldiff(
    old_html: _Element | _AnyStr,
    new_html: _Element | _AnyStr,
) -> str: ...
