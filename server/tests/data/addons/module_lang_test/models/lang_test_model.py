from odoo import fields, models


class LangTestModel(models.Model):
    _name = 'lang_test.item'
    _description = 'Language Test Item'

    name = fields.Char(string='Name', required=True, translate=True)
