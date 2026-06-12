from odoo import fields, models


class ParentModel(models.Model):
    _name = 'x_parent_model'

    parent_only_field = fields.Char()


class DelegatingModel(models.Model):
    _name = 'x_delegating_model'
    _inherits = {'x_parent_model': 'parent_id'}

    parent_id = fields.Many2one('x_parent_model')
    own_field = fields.Char()
