# Every assertion of tests/test_semantic_tokens.rs points at this file. Two rules when editing:
# keep it ASCII-only, and keep each asserted identifier unique on its line -- the test helper
# locates a token by searching for its name on a line.
from odoo import api, fields, models

from . import sem_pkg
from . import sem_tokens_shim
from .sem_tokens_defs import SEM_CONSTANT, PlainClass, module_level_func


def free_function(alpha, beta=1, *star_args, kw_only=2, **star_kwargs):
    return alpha


def pos_only_function(positional, /, standard):
    return standard


def annotated_function(with_annotation: PlainClass):
    return with_annotation


lambda_holder = lambda lambda_param: (
    lambda_param
)


class SemTokensUsage(models.Model):
    _name = 'sem.tokens.usage'
    _description = 'Semantic Tokens Usage'

    name = fields.Char()
    other_id = fields.Many2one('sem.tokens.other')
    total = fields.Float(compute='_compute_total')
    mirrored = fields.Char(related='other_id.other_name')

    @api.depends('other_id.other_name')
    def _compute_total(self):
        self.total = 0.0

    @api.model
    def model_decorated(self):
        return self

    @property
    def own_property(self):
        return 1

    @staticmethod
    def own_static():
        return 1

    @classmethod
    def own_class_method(cls):
        return 1

    def plain_names(self, param):
        local_value = SEM_CONSTANT
        module_level_func(local_value)
        return param

    def attribute_access(self, record):
        instance = PlainClass()
        instance.instance_method(1)
        PlainClass.static_helper(2)
        PlainClass.class_helper(3)
        _ = instance.computed_property
        _ = PlainClass.class_attribute
        _ = self.name
        _ = self.other_id
        _ = record.name
        return instance

    def module_access(self):
        _ = sem_pkg.SEM_PKG_CONSTANT
        _ = sem_tokens_shim.PlainClass
        return sem_tokens_shim.module_level_func()

    def string_arguments(self):
        _ = self.env['sem.tokens.other']
        _ = self.env.ref('module_semantic_tokens.sem_token_record')
        _ = self.env.ref('module_semantic_tokens.no_such_record')
        _ = self.search([('name', '=', 'a value')])
        return 'not.a.model'
