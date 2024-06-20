#
# Types stub for lxml/classlookup.pxi
#

from abc import ABCMeta, abstractmethod
from typing import Mapping, final

from .._types import (
    SupportsLaxedItems,
    _AnyStr,
    _AttrName,
    _ElemClsLookupArg,
    _NSMapArg,
)
from ._element import _Comment, _Element, _Entity, _ProcessingInstruction

#
# Public element classes
#
class ElementBase(_Element):
    """The public Element class

    Original Docstring
    ------------------
    All custom Element classes must inherit from this one.
    To create an Element, use the `Element()` factory.

    BIG FAT WARNING: Subclasses *must not* override `__init__` or
    `__new__` as it is absolutely undefined when these objects will be
    created or destroyed.  All persistent state of Elements must be
    stored in the underlying XML.  If you really need to initialize
    the object after creation, you can implement an ``_init(self)``
    method that will be called directly after object creation.

    Subclasses of this class can be instantiated to create a new
    Element.  By default, the tag name will be the class name and the
    namespace will be empty.  You can modify this with the following
    class attributes:

    * TAG - the tag name, possibly containing a namespace in Clark
      notation

    * NAMESPACE - the default namespace URI, unless provided as part
      of the TAG attribute.

    * HTML - flag if the class is an HTML tag, as opposed to an XML
      tag.  This only applies to un-namespaced tags and defaults to
      false (i.e. XML).

    * PARSER - the parser that provides the configuration for the
      newly created document.  Providing an HTML parser here will
      default to creating an HTML element.

    In user code, the latter three are commonly inherited in class
    hierarchies that implement a common namespace.
    """

    @final
    def __init__(
        self,
        *children: object,
        attrib: SupportsLaxedItems[str, _AnyStr] | None = None,
        nsmap: _NSMapArg | None = None,
        **_extra: _AnyStr,
    ) -> None: ...
    def _init(self) -> None: ...

class CommentBase(_Comment):
    """All custom Comment classes must inherit from this one

    Original Docstring
    ------------------
    To create an XML Comment instance, use the ``Comment()`` factory.

    Subclasses *must not* override `__init__` or `__new__` as it is
    absolutely undefined when these objects will be created or
    destroyed.  All persistent state of Comments must be stored in the
    underlying XML.  If you really need to initialize the object after
    creation, you can implement an ``_init(self)`` method that will be
    called after object creation.
    """

    @final
    def __init__(self, text: _AnyStr | None) -> None: ...
    def _init(self) -> None: ...

class PIBase(_ProcessingInstruction):
    """All custom Processing Instruction classes must inherit from this one.

    Original Docstring
    ------------------
    To create an XML ProcessingInstruction instance, use the ``PI()``
    factory.

    Subclasses *must not* override `__init__` or `__new__` as it is
    absolutely undefined when these objects will be created or
    destroyed.  All persistent state of PIs must be stored in the
    underlying XML.  If you really need to initialize the object after
    creation, you can implement an ``_init(self)`` method that will be
    called after object creation.
    """

    @final
    def __init__(self, target: _AnyStr, text: _AnyStr | None = None) -> None: ...
    def _init(self) -> None: ...

class EntityBase(_Entity):
    """All custom Entity classes must inherit from this one.

    To create an XML Entity instance, use the ``Entity()`` factory.

    Subclasses *must not* override `__init__` or `__new__` as it is
    absolutely undefined when these objects will be created or
    destroyed.  All persistent state of Entities must be stored in the
    underlying XML.  If you really need to initialize the object after
    creation, you can implement an ``_init(self)`` method that will be
    called after object creation.
    """

    @final
    def __init__(self, name: _AnyStr) -> None: ...
    def _init(self) -> None: ...

#
# Class lookup mechanism described in
# https://lxml.de/element_classes.html#setting-up-a-class-lookup-scheme
#

class ElementClassLookup:
    """Superclass of Element class lookups"""

class FallbackElementClassLookup(ElementClassLookup):
    """Superclass of Element class lookups with additional fallback"""

    @property
    def fallback(self) -> ElementClassLookup | None: ...
    def __init__(self, fallback: ElementClassLookup | None = None) -> None: ...
    def set_fallback(self, lookup: ElementClassLookup) -> None:
        """Sets the fallback scheme for this lookup method"""

class ElementDefaultClassLookup(ElementClassLookup):
    """Element class lookup scheme that always returns the default Element
    class.

    The keyword arguments ``element``, ``comment``, ``pi`` and ``entity``
    accept the respective Element classes."""

    @property
    def element_class(self) -> type[_Element]: ...
    @property
    def comment_class(self) -> type[_Comment]: ...
    @property
    def pi_class(self) -> type[_ProcessingInstruction]: ...
    @property
    def entity_class(self) -> type[_Entity]: ...
    def __init__(
        self,
        element: type[ElementBase] | None = None,
        comment: type[CommentBase] | None = None,
        pi: type[PIBase] | None = None,
        entity: type[EntityBase] | None = None,
    ) -> None: ...

class AttributeBasedElementClassLookup(FallbackElementClassLookup):
    """Checks an attribute of an Element and looks up the value in a
    class dictionary.

    Arguments
    ---------
    attribute name: str or QName
        '{ns}name' style string or QName helper
    class_mapping: Mapping of str to element types
        Python dict mapping attribute values to Element classes
    fallback: ElementClassLookup, optional
        Optional fallback lookup mechanism

    A None key in the class mapping will be checked if the attribute is
    missing."""

    def __init__(
        self,
        attribute_name: _AttrName,
        class_mapping: (
            Mapping[str, type[_Element]] | Mapping[str | None, type[_Element]]
        ),
        fallback: ElementClassLookup | None = None,
    ) -> None: ...

class ParserBasedElementClassLookup(FallbackElementClassLookup):
    """Element class lookup based on the XML parser"""

# Though Cython has no notion of abstract method, it is giving
# enough hints that all subclasses should implement lookup method
# making it de-facto abstract method
class CustomElementClassLookup(FallbackElementClassLookup, metaclass=ABCMeta):
    """Element class lookup based on a subclass method

    Original Docstring
    ------------------
    You can inherit from this class and override the method:

    ```python
    lookup(self, type, doc, namespace, name)
    ```

    To lookup the element class for a node. Arguments of the method:
    * type:      one of 'element', 'comment', 'PI', 'entity'
    * doc:       document that the node is in
    * namespace: namespace URI of the node (or None for comments/PIs/entities)
    * name:      name of the element/entity, None for comments, target for PIs

    If you return None from this method, the fallback will be called.
    """

    @abstractmethod
    def lookup(
        self,
        type: _ElemClsLookupArg,
        doc: object,  # Internal doc object
        namespace: str | None,
        name: str | None,
    ) -> type[_Element] | None: ...

class PythonElementClassLookup(FallbackElementClassLookup, metaclass=ABCMeta):
    """Element class lookup based on a subclass method

    Original Docstring
    ------------------
    This class lookup scheme allows access to the entire XML tree in
    read-only mode.  To use it, re-implement the ``lookup(self, doc,
    root)`` method in a subclass::

    ```python
    from lxml import etree, pyclasslookup

    class MyElementClass(etree.ElementBase):
        honkey = True

    class MyLookup(pyclasslookup.PythonElementClassLookup):
        def lookup(self, doc, root):
            if root.tag == "sometag":
                return MyElementClass
            else:
                for child in root:
                    if child.tag == "someothertag":
                        return MyElementClass
            # delegate to default
            return None
    ```

    If you return None from this method, the fallback will be called.

    The first argument is the opaque document instance that contains
    the Element.  The second argument is a lightweight Element proxy
    implementation that is only valid during the lookup.  Do not try
    to keep a reference to it.  Once the lookup is done, the proxy
    will be invalid.

    Also, you cannot wrap such a read-only Element in an ElementTree,
    and you must take care not to keep a reference to them outside of
    the `lookup()` method.

    Note that the API of the Element objects is not complete.  It is
    purely read-only and does not support all features of the normal
    `lxml.etree` API (such as XPath, extended slicing or some
    iteration methods).

    See also
    --------
    - [Official documentation](https://lxml.de/element_classes.html)"""

    @abstractmethod
    def lookup(
        self,
        doc: object,
        element: _Element,  # quasi-Element with all attributes read-only
    ) -> type[_Element] | None: ...

def set_element_class_lookup(lookup: ElementClassLookup | None = None) -> None:
    """Set the global element class lookup method

    Original Docstring
    ------------------
    This defines the main entry point for looking up element implementations.
    The standard implementation uses the :class:`ParserBasedElementClassLookup`
    to delegate to different lookup schemes for each parser.

    This should only be changed by applications, not by library packages.
    In most cases, parser specific lookups should be preferred,
    which can be configured via
    :meth:`lxml.etree.XMLParser.set_element_class_lookup`
    (and the same for HTML parsers).

    Globally replacing the element class lookup by something other than a
    :class:`ParserBasedElementClassLookup` will prevent parser specific lookup
    schemes from working. Several tools rely on parser specific lookups,
    including :mod:`lxml.html` and :mod:`lxml.objectify`.
    """
