#
# Pytype management and annotation
#

from typing import Any, Callable, Iterable

from .._types import _AnyStr, _ElementOrTree
from ._element import ObjectifiedDataElement

class PyType:
    """User defined type

    Origianl Docstring
    ------------------
    Named type that contains a type check function, a type class that
    inherits from `ObjectifiedDataElement` and an optional "stringification"
    function.  The type check must take a string as argument and raise
    `ValueError` or `TypeError` if it cannot handle the string value.  It may be
    None in which case it is not considered for type guessing.  For registered
    named types, the 'stringify' function (or `unicode()` if None) is used to
    convert a Python object with type name 'name' to the string representation
    stored in the XML tree.

    Note that the order in which types are registered matters.  The first
    matching type will be used.

    Example
    -------
    ```python
    PyType('int', int, MyIntClass).register()
    ```

    See Also
    --------
    - [lxml "Python data types" documentation](https://lxml.de/objectify.html#python-data-types)
    """

    @property  # tired of dealing with bytes
    def xmlSchemaTypes(self) -> list[str]: ...
    @xmlSchemaTypes.setter
    def xmlSchemaTypes(self, types: Iterable[str]) -> None: ...
    @property
    def name(self) -> str: ...
    @property
    def type_check(self) -> Callable[[Any], None]: ...
    @property
    def stringify(self) -> Callable[[Any], str]: ...
    def __init__(
        self,
        name: _AnyStr,
        type_check: Callable[[Any], None] | None,
        type_class: type[ObjectifiedDataElement],
        stringify: Callable[[Any], str] | None = None,
    ) -> None: ...
    def register(
        self,
        before: Iterable[str] | None = None,
        after: Iterable[str] | None = None,
    ) -> None: ...
    def unregister(self) -> None: ...

def set_pytype_attribute_tag(attribute_tag: str | None = None) -> None:
    """Change name and namespace of the XML attribute that holds Python
    type information

    Original Docstring
    ------------------
    Do not use this unless you know what you are doing.

    Parameters
    ----------
    attribute_tag: str, optional
        Clark notation namespace and tag of `pytype` attribute. Default is None,
        which means the default value
        `"{http://codespeak.net/lxml/objectify/pytype}pytype"`
    """

def pytypename(obj: object) -> str:
    """Find the name of the corresponding PyType for a Python object"""

def getRegisteredTypes() -> list[PyType]:
    """Returns a list of the currently registered PyType objects

    Original Docstring
    ------------------
    To add a new type, retrieve this list and call `unregister()` for all
    entries.  Then add the new type at a suitable position (possibly replacing
    an existing one) and call `register()` for all entries.

    This is necessary if the new type interferes with the type check functions
    of existing ones (normally only int/float/bool) and must the tried before
    other types.  To add a type that is not yet parsable by the current type
    check functions, you can simply `register()` it, which will append it to the
    end of the type list.
    """

def pyannotate(
    element_or_tree: _ElementOrTree,
    *,
    ignore_old: bool = False,
    ignore_xsi: bool = False,
    empty_pytype: _AnyStr | None = None,
) -> None:
    """Recursively annotates elements of an XML tree with `py:pytype` attributes

    Parameters
    ----------
    element_or_tree: `_Element` or `_ElementTree`
        The XML Element or XML Tree to be precessed
    ignore_old: bool, optional
        If True, current `py:pytype` attributes will always be replaced.
        Otherwise, they will be checked and only replaced if they no longer
        fit the current text value. Default is False, which means checking is done.
    ignore_xsi: bool, optional
        If True, `xsi:type` annotations are completely ignored during element
        type determination. If False (which is default), use them as initial hint.
    empty_pytype: str or bytes, optioanl
        Sets the default pytype annotation of empty elements. Pass 'str',
        for example, to annotate them as string elements. Default is None,
        which means not to process empty elements at all.
    """

def xsiannotate(
    element_or_tree: _ElementOrTree,
    *,
    ignore_old: bool = False,
    ignore_pytype: bool = False,
    empty_type: _AnyStr | None = None,
) -> None:
    """Recursively annotates elements of an XML tree with `xsi:type` attributes

    Note that the mapping from Python types to XSI types is usually ambiguous.
    Currently, only the first XSI type name in the corresponding PyType
    definition will be used for annotation.  Thus, you should consider naming
    the widest type first if you define additional types.

    Parameters
    ----------
    element_or_tree: `_Element` or `_ElementTree`
        The XML Element or XML Tree to be precessed
    ignore_old: bool, optional
        If True, current `xsi:type` attributes will always be replaced.
        Otherwise, they will be checked and only replaced if they no longer
        fit the current text value. Default is False, which means checking is done.
    ignore_pytype:
        If True, `py:pytype` annotations are completely ignored during element
        type determination. If False (which is default), use them as initial hint.
    empty_pytype: str or bytes, optioanl
        Sets the default `xsi:type` attribute of empty elements.
        Pass 'string', for example, to annotate them as string elements. Default
        is None, which means not to process empty elements at all. In particular,
        `xsi:nil` attribute is not added.
    """

def annotate(
    element_or_tree: _ElementOrTree,
    *,
    ignore_old: bool = True,
    ignore_xsi: bool = False,
    empty_pytype: _AnyStr | None = None,
    empty_type: _AnyStr | None = None,
    # following arguments are typed 'bint' in source
    annotate_xsi: bool = False,
    annotate_pytype: bool = True,
) -> None:
    """Recursively annotates elements of an XML tree with `py:pytype`
    and/or `xsi:type` attributes

    Annotation notice
    -----------------
    This function serves as a basis of both `pyannotate()` and
    `xsiannotate()` functions. Beware that `annotate_xsi` and
    `annotate_pytype` parameter type deviates from documentation,
    which is marked as having default value 0 and 1 respectively.
    The underlying internal function uses type `bint` (which means
    bool for Cython). The parameters do act as feature on/off toggle.

    Parameters
    ----------
    element_or_tree: `_Element` or `_ElementTree`
        The XML Element or XML Tree to be precessed
    ignore_old: bool, optional
        If True, current `py:pytype` attributes will always be replaced.
        Otherwise, they will be checked and only replaced if they no longer
        fit the current text value. Default is False, which means checking is done.
    ignore_xsi: bool, optional
        If True, `xsi:type` annotations are completely ignored during element
        type determination. If False (which is default), use them as initial hint.
    empty_pytype: str or bytes, optioanl
        Sets the default pytype annotation of empty elements. Pass `str`,
        for example, to annotate them as string elements. Default is None,
        which means not to process empty elements at all.
    empty_type: str or bytes, optioanl
        Sets the default `xsi:type` annotation of empty elements. Pass `string`,
        for example, to annotate them as string elements. Default is None,
        which means not to process empty elements at all.
    annotate_xsi: bool, optional
        Determines if `xsi:type` annotations would be updated or created,
        default is no (False).
    annotate_pytype: bool, optional
        Determines if `py:pytype` annotations would be updated or created,
        default is yes (True).
    """

def deannotate(
    element_or_tree: _ElementOrTree,
    *,
    pytype: bool = True,
    xsi: bool = True,
    xsi_nil: bool = False,
    cleanup_namespaces: bool = False,
) -> None:
    """Recursively de-annotate elements of an XML tree

    This is achieved by removing `py:pytype`, `xsi:type` and/or `xsi:nil` attributes.

    Parameters
    ----------
    element_or_tree: `_Element` or `_ElementTree`
        The XML Element or XML Tree to be precessed
    pytype: bool, optional
        Whether `py:pytype` attributes should be removed, default is True
    xsi: bool, optional
        Whether `xsi:type` attributes should be removed, default is True
    xsi_nil: bool, optional
        Whether `xsi:nil` attributes should be removed, default is False
    cleanup_namespaces: bool, optional
        Controls if unused namespace declarations should be removed from
        XML tree, default is False
    """
