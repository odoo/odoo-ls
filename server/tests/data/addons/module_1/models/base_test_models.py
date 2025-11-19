from odoo import api, fields, models, _, tools
from odoo.addons.module_1.constants import CONSTANT_1, CONSTANT_2

class BaseTestModel(models.Model):
    _name = "pygls.tests.base_test_model"
    _inherit = []
    _description = "Base Test Model"

    test_int = fields.Integer(compute="_compute_something")
    partner_id = fields.Many2one("res.partner")
    partner_country_phone_code = fields.Integer(related="partner_id.country_id.phone_code", store=True)
    diagnostics_ids = fields.Many2one("module_1.diagnostics_model")

    def get_test_int(self):
        self.ensure_one()
        return self.test_int

    def get_constant(self):
        return CONSTANT_1 + CONSTANT_2

    def for_func(self):
        for var in self:
            print(var)

    @api.onchange("test_int")
    def onchange_test_int(self):
        pass

    @api.depends("partner_id.country_id.code")
    def _compute_something(self):
        self.env["res.partner"]
        self.env["pygls.tests.base_test_model"]
        self.search([("partner_id.country_id.code", ">", 0)])

BaseOtherName = BaseTestModel
baseInstance1 = BaseTestModel()
baseInstance2 = BaseOtherName()
ref_funcBase1 = BaseTestModel.get_test_int
ref_funcBase2 = baseInstance1.get_test_int
return_funcBase2 = baseInstance2.get_test_int()

class NoBaseModel(models.Model):
    _inherit = "module_1.no_base_model"