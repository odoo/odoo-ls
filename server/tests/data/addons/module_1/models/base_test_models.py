from odoo import api, fields, models, _, tools
from odoo.addons.module_1.constants import CONSTANT_1, CONSTANT_2

class BaseTestModel(models.Model):
    _name = "pygls.tests.base_test_model"
    _inherit = []
    _description = "Base Test Model"

    test_int = fields.Integer()

    def get_test_int(self):
        self.ensure_one()
        return self.test_int

    def get_constant(self):
        return CONSTANT_1 + CONSTANT_2