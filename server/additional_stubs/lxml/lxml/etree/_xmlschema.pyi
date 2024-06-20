from typing import overload

from .._types import _ElementOrTree, _FileReadSource
from ._module_misc import LxmlError, _Validator

class XMLSchemaError(LxmlError): ...
class XMLSchemaParseError(XMLSchemaError): ...
class XMLSchemaValidateError(XMLSchemaError): ...

class XMLSchema(_Validator):
    # file arg only useful when etree arg is None
    @overload
    def __init__(
        self,
        etree: _ElementOrTree,
        *,
        file: None = None,
        attribute_defaults: bool = False,
    ) -> None: ...
    @overload
    def __init__(
        self,
        etree: None = None,
        *,
        file: _FileReadSource,
        attribute_defaults: bool = False,
    ) -> None: ...
    def __call__(self, etree: _ElementOrTree) -> bool: ...
