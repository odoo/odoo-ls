# Definitions used by sem_tokens_main.py. Keep this file ASCII-only: the semantic tokens
# tests equate byte offsets with LSP character offsets.
from odoo import fields, models

SEM_CONSTANT = 42


def module_level_func(value):
    return value


class PlainClass:

    class_attribute = 1

    def instance_method(self, value):
        return value

    @staticmethod
    def static_helper(value):
        return value

    @classmethod
    def class_helper(cls, value):
        return value

    @property
    def computed_property(self):
        return 1


class SemTokensOther(models.Model):
    _name = 'sem.tokens.other'
    _description = 'Semantic Tokens Other'

    other_name = fields.Char()
