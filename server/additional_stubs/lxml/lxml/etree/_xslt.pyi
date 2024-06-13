#
# Includes both xslt.pxi and xsltext.pxi
#

import abc
import sys
from typing import Any, ClassVar, Literal, TypedDict, final, overload

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias

if sys.version_info >= (3, 13):
    from warnings import deprecated
else:
    from typing_extensions import deprecated

from .._types import (
    SupportsLaxedItems,
    _AnyStr,
    _DefEtreeParsers,
    _ElementOrTree,
    _FileWriteSource,
)
from ._classlookup import PIBase
from ._element import _Element, _ElementTree
from ._module_misc import LxmlError
from ._serializer import SerialisationError
from ._xmlerror import _ListErrorLog
from ._xpath import XPath

_Stylesheet_Param: TypeAlias = _XSLTQuotedStringParam | XPath | str

# exported constants
LIBXSLT_VERSION: tuple[int, int, int]
LIBXSLT_COMPILED_VERSION: tuple[int, int, int]

class XSLTError(LxmlError):
    """Base class of all XSLT errors"""

class XSLTParseError(XSLTError):
    """Error parsing a stylesheet document"""

class XSLTApplyError(XSLTError):
    """Error running an XSL transformation"""

class XSLTSaveError(XSLTError, SerialisationError):
    """Error serialising an XSLT result"""

class XSLTExtensionError(XSLTError):
    """Error registering an XSLT extension"""

@final
class _XSLTResultTree(_ElementTree):
    """The result of an XSLT evaluation"""

    def write_output(self, file: _FileWriteSource, *, compression: int = 0) -> None:
        """Serialise the XSLT output to a file or file-like object

        As opposed to the generic ``.write()`` method, ``.write_output()`` serialises
        the result as defined by the ``<xsl:output>`` tag.
        """
    @property
    def xslt_profile(self) -> _ElementTree | None:
        """Return an ElementTree with profiling data for the stylesheet run"""

@final
class _XSLTQuotedStringParam:
    """A wrapper class for literal XSLT string parameters that require
    quote escaping"""

    strval: bytes

class __AccessControlConfig(TypedDict):
    read_file: bool | None
    write_file: bool | None
    create_dir: bool | None
    read_network: bool | None
    write_network: bool | None

class XSLTAccessControl:
    """Access control for XSLT: reading/writing files, directories and
    network I/O.

    Access to a type of resource is granted or denied by
    passing any of the following boolean keyword arguments.  All of
    them default to True to allow access.

    - read_file
    - write_file
    - create_dir
    - read_network
    - write_network

    For convenience, there is also a class member `DENY_ALL` that
    provides an XSLTAccessControl instance that is readily configured
    to deny everything, and a `DENY_WRITE` member that denies all
    write access but allows read access.
    """

    DENY_ALL: ClassVar[XSLTAccessControl]
    DENY_WRITE: ClassVar[XSLTAccessControl]

    def __init__(
        self,
        *,
        read_file: bool = True,
        write_file: bool = True,
        create_dir: bool = True,
        read_network: bool = True,
        write_network: bool = True,
    ) -> None: ...
    @property
    def options(self) -> __AccessControlConfig: ...

class XSLT:
    """Turn an XSL document into an XSLT object.

    Calling this object on a tree or Element will execute the XSLT::

        transform = etree.XSLT(xsl_tree)
        result = transform(xml_tree)

    Keyword arguments of the constructor:

    - extensions: a dict mapping ``(namespace, name)`` pairs to
      extension functions or extension elements
    - regexp: enable exslt regular expression support in XPath
      (default: True)
    - access_control: access restrictions for network or file
      system (see `XSLTAccessControl`)

    Keyword arguments of the XSLT call:

    - profile_run: enable XSLT profiling and make the profile available
      as XML document in ``result.xslt_profile`` (default: False)

    Other keyword arguments of the call are passed to the stylesheet
    as parameters.
    """

    def __init__(
        self,
        xslt_input: _ElementOrTree,
        extensions: (
            SupportsLaxedItems[tuple[_AnyStr, _AnyStr], XSLTExtension] | None
        ) = None,
        regexp: bool = True,
        access_control: XSLTAccessControl | None = None,
    ) -> None: ...
    def __call__(
        self,
        _input: _ElementOrTree,
        /,
        profile_run: bool = False,
        **__kw: _Stylesheet_Param,
    ) -> _XSLTResultTree: ...
    @property
    def error_log(self) -> _ListErrorLog: ...
    @staticmethod
    def strparam(strval: _AnyStr) -> _XSLTQuotedStringParam: ...
    @staticmethod
    def set_global_max_depth(max_depth: int) -> None: ...
    @deprecated("Removed since 5.0; call instance directly instead")
    def apply(
        self,
        _input: _ElementOrTree,
        /,
        profile_run: bool = False,
        **__kw: _Stylesheet_Param,
    ) -> _XSLTResultTree: ...
    @deprecated("Since v2.0 (2008); use str(result_tree) instead")
    def tostring(
        self,
        result_tree: _ElementTree,
    ) -> str: ...

class _XSLTProcessingInstruction(PIBase):
    def parseXSL(self, parser: _DefEtreeParsers | None = None) -> _ElementTree: ...
    def set(self, key: Literal["href"], value: str) -> None: ...  # type: ignore[override]

# Nodes are usually some opaque or read-only wrapper of _Element.
# They provide access of varying attributes depending on node type,
# which are not known to static typing. So use typing.Any here
# to not prevent their access.
class XSLTExtension(metaclass=abc.ABCMeta):
    """Base class of an XSLT extension element"""

    @abc.abstractmethod
    def execute(
        self,
        context: Any,  # _XSLTContext,
        self_node: Any,
        input_node: Any,
        output_parent: _Element | None,
    ) -> None:
        """Execute this extension element

        Original Docstring
        ------------------
        Subclasses must override this method.  They may append
        elements to the `output_parent` element here, or set its text
        content.  To this end, the `input_node` provides read-only
        access to the current node in the input document, and the
        `self_node` points to the extension element in the stylesheet.

        Note that the `output_parent` parameter may be `None` if there
        is no parent element in the current context (e.g. no content
        was added to the output tree yet).
        """
    @overload
    def apply_templates(
        self,
        context: Any,  # _XSLTContext,
        node: Any,
        output_parent: _Element,
        *,
        elements_only: bool = False,
        remove_blank_text: bool = False,
    ) -> None: ...
    @overload
    def apply_templates(  # type: ignore[overload-overlap]
        self,
        context: Any,
        node: Any,
        output_parent: None = None,
        *,
        elements_only: Literal[True],
        remove_blank_text: bool = False,
    ) -> list[_Element]: ...
    @overload
    def apply_templates(
        self,
        context: Any,
        node: Any,
        output_parent: None = None,
        *,
        elements_only: bool = False,
        remove_blank_text: bool = False,
    ) -> list[str | _Element]:
        """Call this method to retrieve the result of applying templates
        to an element

        Original Docstring
        ------------------
        The return value is a list of elements or text strings that
        were generated by the XSLT processor.  If you pass
        ``elements_only=True``, strings will be discarded from the result
        list.  The option ``remove_blank_text=True`` will only discard
        strings that consist entirely of whitespace (e.g. formatting).
        These options do not apply to Elements, only to bare string results.

        If you pass an Element as `output_parent` parameter, the result
        will instead be appended to the element (including attributes
        etc.) and the return value will be `None`.  This is a safe way
        to generate content into the output document directly, without
        having to take care of special values like text or attributes.
        Note that the string discarding options will be ignored in this
        case.
        """
    @overload
    def process_children(
        self,
        context: Any,  # _XSLTContext,
        output_parent: _Element,
        *,
        elements_only: bool = False,
        remove_blank_text: bool = False,
    ) -> None: ...
    @overload
    def process_children(  # type: ignore[overload-overlap]
        self,
        context: Any,
        output_parent: None = None,
        *,
        elements_only: Literal[True],
        remove_blank_text: bool = False,
    ) -> list[_Element]: ...
    @overload
    def process_children(
        self,
        context: Any,
        output_parent: None = None,
        *,
        elements_only: bool = False,
        remove_blank_text: bool = False,
    ) -> list[str | _Element]:
        """Call this method to process the XSLT content of the extension
        element itself.

        Original Docstring
        ------------------
        The return value is a list of elements or text strings that
        were generated by the XSLT processor.  If you pass
        ``elements_only=True``, strings will be discarded from the result
        list.  The option ``remove_blank_text=True`` will only discard
        strings that consist entirely of whitespace (e.g. formatting).
        These options do not apply to Elements, only to bare string results.

        If you pass an Element as `output_parent` parameter, the result
        will instead be appended to the element (including attributes
        etc.) and the return value will be `None`.  This is a safe way
        to generate content into the output document directly, without
        having to take care of special values like text or attributes.
        Note that the string discarding options will be ignored in this
        case.
        """
