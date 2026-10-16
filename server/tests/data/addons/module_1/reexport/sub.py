__all__ = ["TextError"]


class TextError(Exception):
    pass


def to_text(value):
    return str(value)
