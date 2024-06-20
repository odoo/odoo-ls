import sys
from typing import (
    Any,
    Callable,
    Collection,
    Iterable,
    Iterator,
    Literal,
    MutableMapping,
    MutableSet,
    TypeVar,
    overload,
)

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias

if sys.version_info >= (3, 11):
    from typing import Never
else:
    from typing_extensions import Never

from .._types import SupportsLaxedItems, _AnyStr
from ._element import HtmlElement

_T = TypeVar("_T")

_FormValues: TypeAlias = list[tuple[str, str]]
_AnyInputElement: TypeAlias = InputElement | SelectElement | TextareaElement

class FormElement(HtmlElement):
    @property
    def inputs(self) -> InputGetter: ...
    @property
    def fields(self) -> FieldsDict: ...
    @fields.setter
    def fields(self, __v: SupportsLaxedItems[str, str]) -> None: ...
    action: str
    method: str
    def form_values(self) -> _FormValues: ...

# FieldsDict is actually MutableMapping *sans* __delitem__
# However it is much simpler to keep MutableMapping and only
# override __delitem__
class FieldsDict(MutableMapping[str, str]):
    inputs: InputGetter
    def __init__(self, inputs: InputGetter) -> None: ...
    def __getitem__(self, __k: str) -> str: ...
    def __setitem__(self, __k: str, __v: str) -> None: ...
    # Use Never for argument to issue early warning that
    # __delitem__ can't be used
    def __delitem__(self, __k: Never) -> Never: ...  # type: ignore[override]
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...

# Quoting from source: it's unclear if this is a dictionary-like object
# or list-like object
class InputGetter(Collection[_AnyInputElement]):
    form: FormElement
    def __init__(self, form: FormElement) -> None: ...
    # __getitem__ is special here: for checkbox group and radio group,
    # it returns special list-like object instead of HtmlElement
    def __getitem__(
        self, __k: str
    ) -> _AnyInputElement | RadioGroup | CheckboxGroup: ...
    def keys(self) -> list[str]: ...
    def items(
        self,
    ) -> list[tuple[str, _AnyInputElement | RadioGroup | CheckboxGroup]]: ...
    def __contains__(self, __o: object) -> bool: ...
    def __iter__(self) -> Iterator[_AnyInputElement]: ...
    def __len__(self) -> int: ...

class _InputMixin:
    @property
    def name(self) -> str | None: ...
    @name.setter
    def name(self, __v: _AnyStr | None) -> None: ...

class TextareaElement(_InputMixin, HtmlElement):
    value: str

class SelectElement(_InputMixin, HtmlElement):
    multiple: bool
    @property
    def value(self) -> str | MultipleSelectOptions: ...
    @value.setter
    def value(self, value: _AnyStr | Collection[str]) -> None: ...
    @property
    def value_options(self) -> list[str]: ...

class MultipleSelectOptions(MutableSet[str]):
    select: SelectElement
    def __init__(self, select: SelectElement) -> None: ...
    @property
    def options(self) -> Iterator[HtmlElement]: ...
    def __contains__(self, x: object) -> bool: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    def add(  # pyright: ignore[reportIncompatibleMethodOverride]
        self, item: str
    ) -> None: ...
    def remove(  # pyright: ignore[reportIncompatibleMethodOverride]
        self, item: str
    ) -> None: ...
    def discard(  # pyright: ignore[reportIncompatibleMethodOverride]
        self, item: str
    ) -> None: ...

class RadioGroup(list[InputElement]):
    value: str | None
    @property
    def value_options(self) -> list[str]: ...

class CheckboxGroup(list[InputElement]):
    @property
    def value(self) -> CheckboxValues: ...
    @value.setter
    def value(self, __v: Iterable[str]) -> None: ...
    @property
    def value_options(self) -> list[str]: ...

class CheckboxValues(MutableSet[str]):
    group: CheckboxGroup
    def __init__(self, group: CheckboxGroup) -> None: ...
    def __contains__(self, x: object) -> bool: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    def add(self, value: str) -> None: ...
    def discard(  # pyright: ignore[reportIncompatibleMethodOverride]
        self, item: str
    ) -> None: ...

class InputElement(_InputMixin, HtmlElement):
    type: str
    value: str | None
    checked: bool
    @property
    def checkable(self) -> bool: ...

class LabelElement(HtmlElement):
    @property
    def for_element(self) -> HtmlElement | None: ...
    @for_element.setter
    def for_element(self, __v: HtmlElement) -> None: ...

# open_http argument has signature (method, url, values) -> Any
@overload
def submit_form(
    form: FormElement,
    extra_values: _FormValues | SupportsLaxedItems[str, str] | None = None,
    open_http: None = None,
) -> Any: ...  # See typeshed _UrlOpenRet
@overload  # open_http as positional argument
def submit_form(
    form: FormElement,
    extra_values: _FormValues | SupportsLaxedItems[str, str] | None,
    open_http: Callable[[Literal["GET", "POST"], str, _FormValues], _T],
) -> _T: ...
@overload  # open_http as keyword argument
def submit_form(
    form: FormElement,
    extra_values: _FormValues | SupportsLaxedItems[str, str] | None = None,
    *,
    open_http: Callable[[Literal["GET", "POST"], str, _FormValues], _T],
) -> _T: ...

# No need to annotate open_http_urllib.
# Only intended as callback func object in submit_form() argument,
# and already used as default if open_http argument is absent.
