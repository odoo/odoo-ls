import sys

if sys.version_info >= (3, 11):
    from typing import LiteralString
else:
    from typing_extensions import LiteralString

from ._element import (
    Classes as Classes,
    Element as Element,
    HtmlComment as HtmlComment,
    HtmlElement as HtmlElement,
    HtmlEntity as HtmlEntity,
    HtmlProcessingInstruction as HtmlProcessingInstruction,
)
from ._form import (
    CheckboxGroup as CheckboxGroup,
    CheckboxValues as CheckboxValues,
    FieldsDict as FieldsDict,
    FormElement as FormElement,
    InputElement as InputElement,
    InputGetter as InputGetter,
    LabelElement as LabelElement,
    MultipleSelectOptions as MultipleSelectOptions,
    RadioGroup as RadioGroup,
    SelectElement as SelectElement,
    TextareaElement as TextareaElement,
    submit_form as submit_form,
)
from ._funcs import (
    find_class as find_class,
    find_rel_links as find_rel_links,
    html_to_xhtml as html_to_xhtml,
    iterlinks as iterlinks,
    make_links_absolute as make_links_absolute,
    open_in_browser as open_in_browser,
    resolve_base_href as resolve_base_href,
    rewrite_links as rewrite_links,
    tostring as tostring,
    xhtml_to_html as xhtml_to_html,
)
from ._parse import (
    HtmlElementClassLookup as HtmlElementClassLookup,
    HTMLParser as HTMLParser,
    XHTMLParser as XHTMLParser,
    document_fromstring as document_fromstring,
    fragment_fromstring as fragment_fromstring,
    fragments_fromstring as fragments_fromstring,
    fromstring as fromstring,
    html_parser as html_parser,
    parse as parse,
    xhtml_parser as xhtml_parser,
)

XHTML_NAMESPACE: LiteralString
