from odoo import api, fields, models, _, tools
class BaseTestModel(models.Model):
    _inherit = "pygls.tests.base_test_model"
    test_int = fields.Integer(compute="_compute_something")

    def _compute_something(self):
        return super()._compute_something()

class TestEmptyModel(models.Model):
    _name = "module_2.empty_model"