from typing import Literal, overload

import cssselect as _csel
from cssselect.parser import Function
from cssselect.xpath import XPathExpr

from ._types import _ET, _ElementOrTree, _NonDefaultNSMapArg, _XPathVarArg
from .etree import XPath
from .html import HtmlElement
from .objectify import ObjectifiedElement

_CSSTransArg = LxmlTranslator | Literal["xml", "html", "xhtml"]

SelectorError = _csel.SelectorError
SelectorSyntaxError = _csel.SelectorSyntaxError
ExpressionError = _csel.ExpressionError

class LxmlTranslator(_csel.GenericTranslator):
    def xpath_contains_function(
        self, xpath: XPathExpr, function: Function
    ) -> XPathExpr: ...

class LxmlHTMLTranslator(LxmlTranslator, _csel.HTMLTranslator):
    pass

class CSSSelector(XPath):
    # Although 'css' is implemented as plain attribute, it is
    # meaningless to modify it, because instance is initialized
    # with translated XPath expression, not the CSS expression.
    # Allowing attribute modification would cause confusion as
    # CSS expression and the underlying XPath expression don't
    # match.
    @property
    def css(self) -> str: ...
    def __init__(
        self,
        css: str,
        namespaces: _NonDefaultNSMapArg | None = None,
        translator: _CSSTransArg = "xml",
    ) -> None: ...
    # It is safe to assume cssselect always selects element
    # representable in original element tree, because CSS
    # expression is transformed into XPath via css_to_xpath()
    # which doesn't support pseudo-element by default.
    # OTOH, the overload situation is similar to SubElement();
    # we handle the 2 built-in element families (HtmlElement
    # and ObjectifiedElement), but the rest is up to users.
    @overload
    def __call__(
        self,
        _etree_or_element: _ElementOrTree[ObjectifiedElement],
        /,
        **_variables: _XPathVarArg,
    ) -> list[ObjectifiedElement]: ...
    @overload
    def __call__(
        self,
        _etree_or_element: _ElementOrTree[HtmlElement],
        /,
        **_variables: _XPathVarArg,
    ) -> list[HtmlElement]: ...
    @overload
    def __call__(
        self,
        _etree_or_element: _ElementOrTree[_ET],
        /,
        **_variables: _XPathVarArg,
    ) -> list[_ET]: ...
