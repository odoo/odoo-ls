from typing import AnyStr, Callable, Iterator, Literal, TypeVar, overload

from .._types import _AnyStr, _ElementOrTree, _OutputMethodArg
from ._element import _HANDLE_FAILURES, HtmlElement

_HtmlDoc_T = TypeVar("_HtmlDoc_T", str, bytes, HtmlElement)

# These are HtmlMixin methods converted to standard functions,
# with element or HTML string as first argument followed by all
# pre-existing args. Quoting from source:
#
#   ... the function takes either an element or an HTML string.  It
#   returns whatever the function normally returns, or if the function
#   works in-place (and so returns None) it returns a serialized form
#   of the resulting document.
#
# Special Notes:
# 1. These funcs operate on attributes that only make sense on
#    normal HtmlElements; lxml raises exception otherwise.
# 2. Although extra 'copy' argument is available, it is intended
#    only for internal use by each function, not something to be
#    arbitrarily changed by users, thus not available in stub.
#
# HACK Interesting, a 15+ yrs bug remains undiscovered,
# probably nobody is using them at all?
# All these standalone link funcs make use of _MethodFunc
# internal class in html/__init__.py, which has bug when
# converting input data. If input is not Element, the class
# automatically converts input to Element via fromstring(),
# taking in all keyword args used in link function call.
# Many of these keywords are unknown to fromstring(),
# thus causing Exception. Workaround this using @overload.

@overload
def find_rel_links(
    doc: HtmlElement,
    rel: str,
) -> list[HtmlElement]: ...
@overload
def find_rel_links(
    doc: _AnyStr,
    rel: str,
    /,
) -> list[HtmlElement]: ...
@overload
def find_class(
    doc: HtmlElement,
    class_name: _AnyStr,
) -> list[HtmlElement]: ...
@overload
def find_class(
    doc: _AnyStr,
    class_name: _AnyStr,
    /,
) -> list[HtmlElement]: ...
@overload
def make_links_absolute(
    doc: HtmlElement,
    base_url: str | None = None,
    resolve_base_href: bool = True,
    handle_failures: _HANDLE_FAILURES | None = None,
) -> HtmlElement: ...
@overload
def make_links_absolute(
    doc: AnyStr,
    base_url: str | None = None,
    resolve_base_href: bool = True,
    handle_failures: _HANDLE_FAILURES | None = None,
    /,
) -> AnyStr: ...
@overload
def resolve_base_href(
    doc: HtmlElement,
    handle_failures: _HANDLE_FAILURES | None = None,
) -> HtmlElement: ...
@overload
def resolve_base_href(
    doc: AnyStr,
    handle_failures: _HANDLE_FAILURES | None = None,
    /,
) -> AnyStr: ...
def iterlinks(
    doc: _HtmlDoc_T,
) -> Iterator[tuple[HtmlElement, str | None, str, int]]: ...
@overload
def rewrite_links(
    doc: HtmlElement,
    link_repl_func: Callable[[str], str | None],
    resolve_base_href: bool = True,
    base_href: str | None = None,
) -> HtmlElement: ...
@overload
def rewrite_links(
    doc: AnyStr,
    link_repl_func: Callable[[str], str | None],
    resolve_base_href: bool = True,
    base_href: str | None = None,
    /,
) -> AnyStr: ...

#
# Tree conversion
#
def html_to_xhtml(html: _ElementOrTree[HtmlElement]) -> None: ...
def xhtml_to_html(xhtml: _ElementOrTree[HtmlElement]) -> None: ...

#
# Tree output
#
# 1. Encoding issue is similar to etree.tostring().
#
# 2. Unlike etree.tostring(), all arguments here are not explicitly
#    keyword-only. Using overload with no default value would be
#    impossible, as the two arguments before it has default value.
#    Need to make a choice here: enforce all arguments to be keyword-only.
#    Less liberal code, but easier to maintain in long term for users.
#
# 3. Although html.tostring() does not forbid method="c14n" (or c14n2),
#    calling tostring() this way would render almost all keyword arguments
#    useless, defeating the purpose of existence of html.tostring().
#    Besides, no c14n specific arguments are accepted here, so it is
#    better to let etree.tostring() handle C14N.
@overload  # encoding=str / "unicode"
def tostring(  # type: ignore[overload-overlap]
    doc: _ElementOrTree[HtmlElement],
    *,
    pretty_print: bool = False,
    include_meta_content_type: bool = False,
    encoding: type[str] | Literal["unicode"],
    method: _OutputMethodArg = "html",
    with_tail: bool = True,
    doctype: str | None = None,
) -> str: ...
@overload  # encoding="..." / None, no encoding arg
def tostring(
    doc: _ElementOrTree[HtmlElement],
    *,
    pretty_print: bool = False,
    include_meta_content_type: bool = False,
    encoding: str | None = None,
    method: _OutputMethodArg = "html",
    with_tail: bool = True,
    doctype: str | None = None,
) -> bytes: ...

#
# Debug
#
def open_in_browser(
    doc: _ElementOrTree[HtmlElement], encoding: str | type[str] | None = None
) -> None: ...
