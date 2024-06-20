from typing import Collection, Iterable, overload

from .._types import _AnyStr, _ElementOrTree, _NSMapArg, _TagSelector

def cleanup_namespaces(
    tree_or_element: _ElementOrTree,
    top_nsmap: _NSMapArg | None = None,
    keep_ns_prefixes: Iterable[_AnyStr] | None = None,
) -> None: ...

# For functions below, the first `tree_or_element` argument
# can never be keyword argument, since tag/attribute names
# that followed are considered positional arguments in
# all possible function signature overloads.

@overload
def strip_attributes(
    __tree_or_elem: _ElementOrTree,
    *attribute_names: str,
) -> None: ...
@overload
def strip_attributes(
    __tree_or_elem: _ElementOrTree, __attrib: Collection[str], /
) -> None: ...
@overload
def strip_elements(
    __tree_or_elem: _ElementOrTree,
    *tag_names: _TagSelector,
    with_tail: bool = True,
) -> None: ...
@overload
def strip_elements(
    __tree_or_elem: _ElementOrTree,
    __tag: Collection[_TagSelector],
    /,
    with_tail: bool = True,
) -> None: ...
@overload
def strip_tags(
    __tree_or_elem: _ElementOrTree,
    *tag_names: _TagSelector,
) -> None: ...
@overload
def strip_tags(
    __tree_or_elem: _ElementOrTree, __tag: Collection[_TagSelector], /
) -> None: ...
