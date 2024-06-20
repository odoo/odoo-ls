from abc import abstractmethod
from typing import Callable, Mapping, Protocol, TypeVar

from .._types import _DefEtreeParsers, _ElementFactory
from ._element import _Attrib, _Comment, _Element, _ProcessingInstruction
from ._parser import XMLSyntaxError

_T_co = TypeVar("_T_co", covariant=True)

class XMLSyntaxAssertionError(XMLSyntaxError, AssertionError): ...

class ParserTarget(Protocol[_T_co]):
    """This is a stub-only class representing parser target interface.

    - Because almost all methods are optional, ParserTarget should be
      explicitly inherited in code for type checking. See TreeBuilder
      and the snippet example below.
    - Some IDEs can do method signature autocompletion. See notes below.

    Example
    -------
    ```python
    from lxml import etree
    if not TYPE_CHECKING:
        etree.ParserTarget = object

    class MyParserTarget(etree.ParserTarget):
        def __init__(self) -> None: ...
        def start(self,  # 3 argument form is not autocompleted
            tag: str, attrib: _Attrib, nsmap: Mapping[str, str] = ...,
        ) -> None: ...
            # Do something
        def close(self) -> str:
            return "something"

    parser = etree.HTMLParser()  # type is HTMLParser[_Element]
    result = parser.close()  # _Element

    t1 = MyParserTarget()
    parser = etree.HTMLParser(target=t1)  # mypy -> HTMLParser[Any]
                                          # pyright -> HTMLParser[Unknown]

    t2 = cast("etree.ParserTarget[str]", MyParserTarget())
    parser = etree.HTMLParser(target=t2)  # HTMLParser[str]
    result = parser.close()  # str
    ```

    Notes
    -----
    - Only `close()` is mandatory. In extreme case, a vanilla class instance
      with noop `close()` is a valid null parser target that does nothing.
    - `start()` can take either 2 or 3 extra arguments.
    - Some methods are undocumented. They are included in stub nonetheless.

    See Also
    --------
    - `_PythonSaxParserTarget()` in `src/lxml/parsertarget.pxi`
    - [Target parser official document](https://lxml.de/parsing.html#the-target-parser-interface)
    """

    @abstractmethod
    def close(self) -> _T_co: ...
    def comment(self, text: str) -> None: ...
    def data(self, data: str) -> None: ...
    def end(self, tag: str) -> None: ...
    def start(
        self,
        tag: str,
        attrib: _Attrib | Mapping[str, str] | None,
        nsmap: Mapping[str, str] | None = None,
    ) -> None: ...
    # Methods below are undocumented. Lxml has described
    # 'start-ns' and 'end-ns' events however.
    def pi(self, target: str, data: str | None) -> None: ...
    # Default namespace prefix is empty string, not None
    def start_ns(self, prefix: str, uri: str) -> None: ...
    def end_ns(self, prefix: str) -> None: ...
    def doctype(
        self,
        root_tag: str | None,
        public_id: str | None,
        system_id: str | None,
    ) -> None: ...

class TreeBuilder(ParserTarget[_Element]):
    def __init__(
        self,
        *,
        element_factory: _ElementFactory[_Element] | None = None,
        parser: _DefEtreeParsers | None = None,
        comment_factory: Callable[..., _Comment] | None = None,
        pi_factory: Callable[..., _ProcessingInstruction] | None = None,
        insert_comments: bool = True,
        insert_pis: bool = True,
    ) -> None: ...
    def close(self) -> _Element: ...
