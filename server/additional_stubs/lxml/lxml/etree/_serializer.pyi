from types import TracebackType
from typing import (
    Any,
    AsyncContextManager,
    Callable,
    ContextManager,
    Iterable,
    final,
    overload,
)

from _typeshed import SupportsWrite

from .._types import (
    SupportsLaxedItems,
    _AnyStr,
    _ElementOrTree,
    _FileReadSource,
    _FileWriteSource,
    _NSMapArg,
    _OutputMethodArg,
    _TagName,
)
from ._element import _Element
from ._module_misc import LxmlError

class SerialisationError(LxmlError): ...

# Interface quite similar to a ParserTarget, but canonicalized output
# is written during various stages before calling .close()
class C14NWriterTarget:
    def __init__(
        self,
        write: Callable[[str], Any],
        *,
        with_comments: bool = False,
        strip_text: bool = False,
        rewrite_prefixes: bool = False,
        qname_aware_tags: Iterable[str] | None = None,
        qname_aware_attrs: Iterable[str] | None = None,
        exclude_attrs: Iterable[str] | None = None,
        exclude_tags: Iterable[str] | None = None,
    ) -> None: ...
    def data(self, data: str) -> None: ...
    def start_ns(self, prefix: str, uri: str) -> None: ...
    def start(self, tag: str, attrs: SupportsLaxedItems[str, str] | None) -> None: ...
    def end(self, tag: str) -> None: ...
    def comment(self, text: str) -> None: ...
    def pi(self, target: str, data: str | None) -> None: ...
    def close(self) -> None: ...

# canonicalize() overload matrix:
# 2x input (via xml_data, via from_file)
# 2x output (None, .write())
# options keyword arguments come from C14NWriterTarget.__init__
@overload
def canonicalize(
    xml_data: _AnyStr | _ElementOrTree,
    *,
    out: SupportsWrite[str],
    with_comments: bool = False,
    strip_text: bool = False,
    rewrite_prefixes: bool = False,
    qname_aware_tags: Iterable[str] | None = None,
    qname_aware_attrs: Iterable[str] | None = None,
    exclude_attrs: Iterable[str] | None = None,
    exclude_tags: Iterable[str] | None = None,
) -> None: ...
@overload
def canonicalize(
    xml_data: None = None,
    *,
    out: SupportsWrite[str],
    from_file: _FileReadSource,
    with_comments: bool = False,
    strip_text: bool = False,
    rewrite_prefixes: bool = False,
    qname_aware_tags: Iterable[str] | None = None,
    qname_aware_attrs: Iterable[str] | None = None,
    exclude_attrs: Iterable[str] | None = None,
    exclude_tags: Iterable[str] | None = None,
) -> None: ...
@overload
def canonicalize(
    xml_data: _AnyStr | _ElementOrTree,
    *,
    out: None = None,
    with_comments: bool = False,
    strip_text: bool = False,
    rewrite_prefixes: bool = False,
    qname_aware_tags: Iterable[str] | None = None,
    qname_aware_attrs: Iterable[str] | None = None,
    exclude_attrs: Iterable[str] | None = None,
    exclude_tags: Iterable[str] | None = None,
) -> str: ...
@overload
def canonicalize(
    xml_data: None = None,
    *,
    out: None = None,
    from_file: _FileReadSource,
    with_comments: bool = False,
    strip_text: bool = False,
    rewrite_prefixes: bool = False,
    qname_aware_tags: Iterable[str] | None = None,
    qname_aware_attrs: Iterable[str] | None = None,
    exclude_attrs: Iterable[str] | None = None,
    exclude_tags: Iterable[str] | None = None,
) -> str: ...

#
# Incremental serializers
# https://lxml.de/api.html#incremental-xml-generation
#
# Special notes:
# 1. .method() argument can accept 'text', but it just behaves like 'xml'
# 2. Both sync/async version of .method() return the same context
#    manager objects. For coherence, we distinguish their return type
#    differently.
#
@final
class _IncrementalFileWriter:
    def write_declaration(
        self,
        version: _AnyStr | None = None,
        standalone: bool | None = None,
        doctype: _AnyStr | None = None,
    ) -> None: ...
    def write_doctype(self, doctype: _AnyStr | None) -> None: ...
    def write(
        self,
        *args: _AnyStr | _Element,
        with_tail: bool = True,
        pretty_print: bool = False,
        method: _OutputMethodArg | None = None,
    ) -> None: ...
    def flush(self) -> None: ...
    def method(self, method: _OutputMethodArg | None) -> ContextManager[None]: ...
    def element(
        self,
        tag: _TagName,
        attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
        nsmap: _NSMapArg | None = None,
        method: _OutputMethodArg | None = None,
        **_extra: _AnyStr,
    ) -> ContextManager[None]: ...

@final
class _AsyncIncrementalFileWriter:
    async def write_declaration(
        self,
        version: _AnyStr | None = None,
        standalone: bool | None = None,
        doctype: _AnyStr | None = None,
    ) -> None: ...
    async def write_doctype(self, doctype: _AnyStr | None) -> None: ...
    async def write(
        self,
        *args: _AnyStr | _Element | None,
        with_tail: bool = True,
        pretty_print: bool = False,
        method: _OutputMethodArg | None = None,
    ) -> None: ...
    async def flush(self) -> None: ...
    def method(self, method: _OutputMethodArg | None) -> AsyncContextManager[None]: ...
    def element(
        self,
        tag: _TagName,
        attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
        nsmap: _NSMapArg | None = None,
        method: _OutputMethodArg | None = None,
        **_extra: _AnyStr,
    ) -> AsyncContextManager[None]: ...

class xmlfile(
    AsyncContextManager[_AsyncIncrementalFileWriter],
    ContextManager[_IncrementalFileWriter],
):
    def __init__(
        self,
        output_file: _FileWriteSource,
        encoding: _AnyStr | None = None,
        compression: int | None = None,
        close: bool = False,
        buffered: bool = True,
    ) -> None: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None: ...
    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None: ...

class htmlfile(xmlfile): ...
