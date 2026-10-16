from .sub import *
from .sub import to_text


def check_text(value):
    if not isinstance(value, str):
        raise TextError(to_text(value))
