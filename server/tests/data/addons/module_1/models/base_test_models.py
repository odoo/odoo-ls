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
    
BaseOtherName = BaseTestModel
baseInstance1 = BaseTestModel()
baseInstance2 = BaseOtherName()
ref_funcBase1 = BaseTestModel.get_test_int
ref_funcBase2 = baseInstance1.get_test_int
return_funcBase2 = baseInstance2.get_test_int()