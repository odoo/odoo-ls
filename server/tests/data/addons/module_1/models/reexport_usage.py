from odoo.addons.module_1 import reexport
from odoo.addons.module_1.reexport import TextError, to_text


def render(value):
    try:
        return to_text(value)
    except TextError:
        return reexport.to_text(value)


def is_text_error(error):
    return isinstance(error, TextError)


def render_twice(value):
    text = to_text(value)
    return text + text
