# `_order` validation, asserted by tests/diagnostics/ols03029.rs: one case per class, found by its
# `_order` line, which must stay unique and ASCII-only.
from odoo import fields, models


class OrderCheck(models.Model):
    _name = 'order_check.record'
    _description = 'Order Check'
    _order = 'name desc, id'

    name = fields.Char()
    parent_id = fields.Many2one('order_check.record')


class OrderCheckUpperCase(models.Model):
    _inherit = 'order_check.record'
    _order = 'name DESC NULLS LAST'


class OrderCheckIdProperty(models.Model):
    _inherit = 'order_check.record'
    _order = 'parent_id.id desc'


class OrderCheckConcatenated(models.Model):
    _inherit = 'order_check.record'
    _order = 'na' 'me'


class OrderCheckEscape(models.Model):
    _inherit = 'order_check.record'
    _order = "name,\tid"


class OrderCheckEmpty(models.Model):
    _inherit = 'order_check.record'
    _order = ''


class OrderCheckUnknownField(models.Model):
    _inherit = 'order_check.record'
    _order = 'unknown_field'


class OrderCheckQuotedField(models.Model):
    _inherit = 'order_check.record'
    _order = '"name"'


class OrderCheckFunc(models.Model):
    _inherit = 'order_check.record'
    _order = 'name:month desc'


class OrderCheckWrongDirection(models.Model):
    _inherit = 'order_check.record'
    _order = 'name dsc'


class OrderCheckNullsAlone(models.Model):
    _inherit = 'order_check.record'
    _order = 'name nulls'


class OrderCheckWrongNulls(models.Model):
    _inherit = 'order_check.record'
    _order = 'name nulls firts'


class OrderCheckDirectionAfterNulls(models.Model):
    _inherit = 'order_check.record'
    _order = 'name nulls first desc'


class OrderCheckNullsSpaces(models.Model):
    _inherit = 'order_check.record'
    _order = 'name nulls  first'


class OrderCheckEmptyItem(models.Model):
    _inherit = 'order_check.record'
    _order = 'name,,id'


class OrderCheckTrailingComma(models.Model):
    _inherit = 'order_check.record'
    _order = 'name,'


class OrderCheckNotMany2one(models.Model):
    _inherit = 'order_check.record'
    _order = 'name.id'


class OrderCheckNotId(models.Model):
    _inherit = 'order_check.record'
    _order = 'parent_id.name'


class OrderCheckMissingField(models.Model):
    _inherit = 'order_check.record'
    _order = '.id desc'
