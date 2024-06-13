import sys
from typing import Any, Iterable, Literal, final, overload

if sys.version_info >= (3, 10):
    from typing import TypeGuard
else:
    from typing_extensions import TypeGuard

if sys.version_info >= (3, 13):
    from warnings import deprecated
else:
    from typing_extensions import deprecated

from .._types import (
    _ET,
    _AnyStr,
    _DefEtreeParsers,
    _ElementOrTree,
    _ET_co,
    _FileReadSource,
    _OutputMethodArg,
)
from ._element import _Element, _ElementTree
from ._parser import HTMLParser, XMLParser

@overload
def HTML(
    text: _AnyStr,
    parser: HTMLParser[_ET_co],
    *,
    base_url: _AnyStr | None = None,
) -> _ET_co: ...
@overload
def HTML(
    text: _AnyStr,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> _Element: ...
@overload
def XML(
    text: _AnyStr,
    parser: XMLParser[_ET_co],
    *,
    base_url: _AnyStr | None = None,
) -> _ET_co: ...
@overload
def XML(
    text: _AnyStr,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> _Element: ...
@overload
def parse(
    source: _FileReadSource,
    parser: _DefEtreeParsers[_ET_co],
    *,
    base_url: _AnyStr | None = None,
) -> _ElementTree[_ET_co]: ...
@overload
def parse(
    source: _FileReadSource,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> _ElementTree: ...
@overload
def fromstring(
    text: _AnyStr,
    parser: _DefEtreeParsers[_ET_co],
    *,
    base_url: _AnyStr | None = None,
) -> _ET_co: ...
@overload
def fromstring(
    text: _AnyStr,
    parser: None = None,
    *,
    base_url: _AnyStr | None = None,
) -> _Element: ...
@overload
def fromstringlist(
    strings: Iterable[_AnyStr],
    parser: _DefEtreeParsers[_ET_co],
) -> _ET_co: ...
@overload
def fromstringlist(
    strings: Iterable[_AnyStr],
    parser: None = None,
) -> _Element: ...

# Under XML Canonicalization (C14N) mode, most arguments are ignored,
# some arguments would even raise exception outright if specified.
@overload  # method="c14n"
def tostring(
    element_or_tree: _ElementOrTree,
    *,
    method: Literal["c14n"],
    exclusive: bool = False,
    inclusive_ns_prefixes: Iterable[_AnyStr] | None = None,
    with_comments: bool = True,
) -> bytes: ...
@overload  # method="c14n2"
def tostring(
    element_or_tree: _ElementOrTree,
    *,
    method: Literal["c14n2"],
    with_comments: bool = True,
    strip_text: bool = False,
) -> bytes: ...
@overload  # Native str, no XML declaration
def tostring(  # type: ignore[overload-overlap]
    element_or_tree: _ElementOrTree,
    *,
    encoding: type[str] | Literal["unicode"],
    method: _OutputMethodArg = "xml",
    pretty_print: bool = False,
    with_tail: bool = True,
    standalone: bool | None = None,
    doctype: str | None = None,
) -> str: ...
@overload  # byte str, no XML declaration
def tostring(
    element_or_tree: _ElementOrTree,
    *,
    encoding: str | None = None,
    method: _OutputMethodArg = "xml",
    xml_declaration: bool | None = None,
    pretty_print: bool = False,
    with_tail: bool = True,
    standalone: bool | None = None,
    doctype: str | None = None,
) -> bytes: ...
def indent(
    element_or_tree: _ElementOrTree,
    space: str = "  ",
    *,
    level: int = 0,
) -> None: ...
@deprecated(
    "For ElementTree 1.3 compat only; result is tostring() output wrapped inside a list"
)
def tostringlist(
    element_or_tree: _ElementOrTree, *args: Any, **__kw: Any
) -> list[str]: ...
@deprecated('Since v3.3.2; use tostring() with encoding="unicode" argument')
def tounicode(
    element_or_tree: _ElementOrTree,
    *,
    method: str = "xml",
    pretty_print: bool = False,
    with_tail: bool = True,
    doctype: str | None = None,
) -> None: ...
def iselement(element: object) -> TypeGuard[_Element]: ...

# HACK PyCapsule needs annotation of ctypes.pythonapi, which has no
# annotation support currently. Use generic object for now.
@overload
def adopt_external_document(
    capsule: object,
    parser: _DefEtreeParsers[_ET],
) -> _ElementTree[_ET]: ...
@overload
def adopt_external_document(
    capsule: object,
    parser: None = None,
) -> _ElementTree:
    """
    Original Docstring
    ------------------
    Unpack a libxml2 document pointer from a PyCapsule and wrap it in an
    lxml ElementTree object.

    This allows external libraries to build XML/HTML trees using libxml2
    and then pass them efficiently into lxml for further processing.

    If a ``parser`` is provided, it will be used for configuring the
    lxml document.  No parsing will be done.

    The capsule must have the name ``"libxml2:xmlDoc"`` and its pointer
    value must reference a correct libxml2 document of type ``xmlDoc*``.
    The creator of the capsule must take care to correctly clean up the
    document using an appropriate capsule destructor.  By default, the
    libxml2 document will be copied to let lxml safely own the memory
    of the internal tree that it uses.

    If the capsule context is non-NULL, it must point to a C string that
    can be compared using ``strcmp()``.  If the context string equals
    ``"destructor:xmlFreeDoc"``, the libxml2 document will not be copied
    but the capsule invalidated instead by clearing its destructor and
    name.  That way, lxml takes ownership of the libxml2 document in memory
    without creating a copy first, and the capsule destructor will not be
    called.  The document will then eventually be cleaned up by lxml using
    the libxml2 API function ``xmlFreeDoc()`` once it is no longer used.

    If no copy is made, later modifications of the tree outside of lxml
    should not be attempted after transferring the ownership.
    """

def register_namespace(prefix: _AnyStr, uri: _AnyStr) -> None:
    """Registers a namespace prefix that newly created Elements in that
    namespace will use.  The registry is global, and any existing
    mapping for either the given prefix or the namespace URI will be
    removed."""

# Debugging only
def dump(elem: _Element, *, pretty_print: bool = True, with_tail: bool = True) -> None:
    """Writes an element tree or element structure to sys.stdout.
    This function should be used for debugging only."""
@final
class _MemDebug:
    """Debugging support for the memory allocation in libxml2"""

    def bytes_used(self) -> int:
        """
        Returns
        -------
        int
            The total amount of memory (in bytes) currently used by libxml2.
            Note that libxml2 constrains this value to a C int, which limits
            the accuracy on 64 bit systems.
        """
    def blocks_used(self) -> int:
        """
        Returns
        -------
        int
            The total number of memory blocks currently allocated by libxml2.
            Note that libxml2 constrains this value to a C int, which limits
            the accuracy on 64 bit systems.
        """
    def dict_size(self) -> int:
        """
        Returns
        -------
        int
            The current size of the global name dictionary used by libxml2
            for the current thread.  Each thread has its own dictionary.
        """
    def dump(
        self, output_file: _AnyStr | None = None, byte_count: int | None = None
    ) -> None:
        """Dumps the current memory blocks allocated by libxml2 to a file

        Parameters
        ----------
        output_file : str or bytes, optional
            Output file path, default is ".memorylist" under current directory
        byte_count : int, optional
            Limits number of bytes in the dump, default is None (unlimited)
        """
    def show(
        self, output_file: _AnyStr | None = None, block_count: int | None = None
    ) -> None:
        """Dumps the current memory blocks allocated by libxml2 to a file
        The output file format is suitable for line diffing.

        Parameters
        ----------
        output_file : str or bytes, optional
            Output file path, default is ".memorydump" under current directory
        block_count : int, optional
            Limits number of blocks in the dump, default is None (unlimited)
        """

memory_debugger: _MemDebug
"""Debugging support for the memory allocation in libxml2"""
