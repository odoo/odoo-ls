from odoo import api, fields, models, _, tools
class BaseTestModel(models.Model):
    _inherit = "pygls.tests.base_test_model"
    test_int = fields.Integer(compute="_compute_something")
    diag_id = fields.Many2one("module_1.diagnostics_model")

    def _compute_something(self):
        return super()._compute_something()

class Module2CustomModel(models.Model):
    _name = "module_2.custom_model"
    _description = "Module 2 Custom Model"

    diag_id = fields.Many2one("module_1.diagnostics_model")

class TestEmptyModel(models.Model):
    _name = "module_2.empty_model"