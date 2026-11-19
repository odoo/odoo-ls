from odoo import fields, models

from .selection_constants import IMPORTED_STATES

STATES = [("draft", "Draft"), ("done", "Done")]


class SelectionModel(models.Model):
    _name = "pygls.tests.selection_model"
    _description = "Selection fields giving their values in every way odoo-ls handles"

    literal = fields.Selection([("a", "A"), ("b", "B")])
    keyword = fields.Selection(selection=[("a", "A")])
    constant = fields.Selection(STATES)
    imported_constant = fields.Selection(IMPORTED_STATES)
    method = fields.Selection("_get_values")
    function = fields.Selection(lambda self: [("a", "A")])
    non_string_key = fields.Selection([(1, "One")])
    reference = fields.Reference(selection=[("res.partner", "Partner")])

    def _get_values(self):
        return [("a", "A")]


class SelectionModelExtension(models.Model):
    _inherit = "pygls.tests.selection_model"

    literal = fields.Selection(selection_add=[("c", "C"), ("a",)])
