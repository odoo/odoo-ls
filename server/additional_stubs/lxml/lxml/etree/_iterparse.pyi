import sys
from typing import Collection, Iterable, Iterator, Literal, TypeVar, overload

from _typeshed import SupportsRead

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias

if sys.version_info >= (3, 11):
    from typing import LiteralString
else:
    from typing_extensions import LiteralString

from .._types import (
    _AnyStr,
    _ElementFactory,
    _ElementOrTree,
    _ET_co,
    _FilePath,
    _SaxEventNames,
    _TagSelector,
)
from ._classlookup import ElementClassLookup
from ._docloader import _ResolverRegistry
from ._element import _Element
from ._xmlerror import _ListErrorLog
from ._xmlschema import XMLSchema

_T_co = TypeVar("_T_co", covariant=True)

# See https://lxml.de/parsing.html#event-types
# Undocumented: 'comment' and 'pi' are actually supported!
_NoNSEventNames: TypeAlias = Literal["start", "end", "comment", "pi"]
_SaxNsEventValues: TypeAlias = tuple[str, str] | None  # for start-ns & end-ns event

class iterparse(Iterator[_T_co]):
    """Incremental parser

    Annotation
    ----------
    Totally 5 function signatures are available:
    - HTML mode (`html=True`), where namespace events are ignored
    - `start`, `end`, `comment` and `pi` events, where only
      Element values are produced
    - XML mode with `start-ns` or `end-ns` events, producing
      namespace tuple (for `start-ns`) or nothing (`end-ns`)
    - Catch-all signature where `events` arg is specified
    - `events` arg absent, implying only `end` event is emitted

    Original Docstring
    ------------------
    Parses XML into a tree and generates tuples (event, element) in a
    SAX-like fashion. ``event`` is any of 'start', 'end', 'start-ns',
    'end-ns'.

    For 'start' and 'end', ``element`` is the Element that the parser just
    found opening or closing.  For 'start-ns', it is a tuple (prefix, URI) of
    a new namespace declaration.  For 'end-ns', it is simply None.  Note that
    all start and end events are guaranteed to be properly nested.

    The keyword argument ``events`` specifies a sequence of event type names
    that should be generated.  By default, only 'end' events will be
    generated.

    The additional ``tag`` argument restricts the 'start' and 'end' events to
    those elements that match the given tag.  The ``tag`` argument can also be
    a sequence of tags to allow matching more than one tag.  By default,
    events are generated for all elements.  Note that the 'start-ns' and
    'end-ns' events are not impacted by this restriction.

    The other keyword arguments in the constructor are mainly based on the
    libxml2 parser configuration.  A DTD will also be loaded if validation or
    attribute default values are requested."""

    @overload  # html mode -> namespace events suppressed
    def __new__(  # type: ignore[overload-overlap]
        cls,
        source: _FilePath | SupportsRead[bytes],
        events: Iterable[_SaxEventNames] = ("end",),
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        remove_blank_text: bool = False,
        compact: bool = True,
        resolve_entities: bool = True,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        encoding: _AnyStr | None = None,
        html: Literal[True],
        recover: bool | None = None,
        huge_tree: bool = False,
        collect_ids: bool = True,
        schema: XMLSchema | None = None,
    ) -> iterparse[tuple[_NoNSEventNames, _Element]]: ...
    @overload  # element-only events
    def __new__(
        cls,
        source: _FilePath | SupportsRead[bytes],
        events: Iterable[_NoNSEventNames],
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        remove_blank_text: bool = False,
        compact: bool = True,
        resolve_entities: bool = True,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        encoding: _AnyStr | None = None,
        html: bool = False,
        recover: bool | None = None,
        huge_tree: bool = False,
        collect_ids: bool = True,
        schema: XMLSchema | None = None,
    ) -> iterparse[tuple[_NoNSEventNames, _Element]]: ...
    @overload  # NS-only events
    def __new__(
        cls,
        source: _FilePath | SupportsRead[bytes],
        events: Iterable[Literal["start-ns", "end-ns"]],
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        remove_blank_text: bool = False,
        compact: bool = True,
        resolve_entities: bool = True,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        encoding: _AnyStr | None = None,
        html: bool = False,
        recover: bool | None = None,
        huge_tree: bool = False,
        collect_ids: bool = True,
        schema: XMLSchema | None = None,
    ) -> iterparse[
        tuple[Literal["start-ns"], tuple[str, str]] | tuple[Literal["end-ns"], None]
    ]: ...
    @overload  # other mixed events
    def __new__(
        cls,
        source: _FilePath | SupportsRead[bytes],
        events: Iterable[_SaxEventNames],
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        remove_blank_text: bool = False,
        compact: bool = True,
        resolve_entities: bool = True,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        encoding: _AnyStr | None = None,
        html: bool = False,
        recover: bool | None = None,
        huge_tree: bool = False,
        collect_ids: bool = True,
        schema: XMLSchema | None = None,
    ) -> iterparse[
        tuple[_NoNSEventNames, _Element]
        | tuple[Literal["start-ns"], tuple[str, str]]
        | tuple[Literal["end-ns"], None]
    ]: ...
    @overload  # events absent -> only 'end' event emitted
    def __new__(
        cls,
        source: _FilePath | SupportsRead[bytes],
        *,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
        attribute_defaults: bool = False,
        dtd_validation: bool = False,
        load_dtd: bool = False,
        no_network: bool = True,
        remove_blank_text: bool = False,
        compact: bool = True,
        resolve_entities: bool = True,
        remove_comments: bool = False,
        remove_pis: bool = False,
        strip_cdata: bool = True,
        encoding: _AnyStr | None = None,
        html: bool = False,
        recover: bool | None = None,
        huge_tree: bool = False,
        collect_ids: bool = True,
        schema: XMLSchema | None = None,
    ) -> iterparse[tuple[Literal["end"], _Element]]: ...
    def __next__(self) -> _T_co: ...
    # root property only present after parsing is done
    @property
    def root(self) -> _Element | None: ...
    @property
    def error_log(self) -> _ListErrorLog: ...
    @property
    def resolvers(self) -> _ResolverRegistry: ...
    @property
    def version(self) -> LiteralString: ...
    def set_element_class_lookup(
        self,
        lookup: ElementClassLookup | None = None,
    ) -> None: ...
    makeelement: _ElementFactory

class iterwalk(Iterator[_T_co]):
    """Tree walker that generates events from an existing tree as if it
    was parsing XML data with ``iterparse()``

    Annotation
    ----------
    Totally 4 function signatures, depending on `events` argument:
    - Default value, where only `end` event is emitted
    - `start`, `end`, `comment` and `pi` events, where only
      Element values are produced
    - Namespace events (`start-ns` or `end-ns`), producing
      namespace tuple (for `start-ns`) or nothing (`end-ns`)
    - Final catch-all for custom events combination


    Original Docstring
    ------------------
    Just as for ``iterparse()``, the ``tag`` argument can be a single tag or a
    sequence of tags.

    After receiving a 'start' or 'start-ns' event, the children and
    descendants of the current element can be excluded from iteration
    by calling the ``skip_subtree()`` method.
    """

    # There is no concept of html mode in iterwalk; namespace events
    # are not suppressed like iterparse()
    @overload  # element-only events
    def __new__(
        cls,
        element_or_tree: _ElementOrTree[_ET_co],
        events: Iterable[_NoNSEventNames],
        tag: _TagSelector | Collection[_TagSelector] | None = None,
    ) -> iterwalk[tuple[_NoNSEventNames, _ET_co]]: ...
    @overload  # namespace-only events
    def __new__(
        cls,
        element_or_tree: _ElementOrTree[_ET_co],
        events: Iterable[Literal["start-ns", "end-ns"]],
        tag: _TagSelector | Collection[_TagSelector] | None = None,
    ) -> iterwalk[
        tuple[Literal["start-ns"], tuple[str, str]] | tuple[Literal["end-ns"], None]
    ]: ...
    @overload  # all other events combination
    def __new__(
        cls,
        element_or_tree: _ElementOrTree[_ET_co],
        events: Iterable[_SaxEventNames],
        tag: _TagSelector | Collection[_TagSelector] | None = None,
    ) -> iterwalk[
        tuple[_NoNSEventNames, _ET_co]
        | tuple[Literal["start-ns"], tuple[str, str]]
        | tuple[Literal["end-ns"], None]
    ]: ...
    @overload  # default events ('end' only)
    def __new__(
        cls,
        element_or_tree: _ElementOrTree[_ET_co],
        /,
        tag: _TagSelector | Collection[_TagSelector] | None = None,
    ) -> iterwalk[tuple[Literal["end"], _ET_co]]: ...
    def __next__(self) -> _T_co: ...
    def skip_subtree(self) -> None: ...
