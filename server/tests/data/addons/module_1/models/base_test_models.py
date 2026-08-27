from odoo import api, fields, models, _, tools
from odoo.addons.module_1.constants import CONSTANT_1, CONSTANT_2

class BaseTestModel(models.Model):
    _name = "pygls.tests.base_test_model"
    _inherit = []
    _description = "Base Test Model"

    test_int = fields.Integer(compute="_compute_something")
    partner_id = fields.Many2one("res.partner")
    partner_country_phone_code = fields.Integer(related="partner_id.country_id.phone_code", store=True)
    diagnostics_id = fields.Many2one("module_1.diagnostics_model")
    same_name_id = fields.Many2one(comodel_name="module_1.same_name_model")

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
        partner = self.search([], limit=2)[-1:]
        self.env.ref("module_1.xml_test_model")
        self.env["pygls.tests.xml_test_model"]

    def _get_partner_id(self):
        partner = self.partner_id
        return partner

BaseOtherName = BaseTestModel
baseInstance1 = BaseTestModel()
baseInstance2 = BaseOtherName()
ref_funcBase1 = BaseTestModel.get_test_int
ref_funcBase2 = baseInstance1.get_test_int
return_funcBase2 = baseInstance2.get_test_int()

class NoBaseModel(models.Model):
    _inherit = "module_1.no_base_model"

basic_var = 42
lambda_ref = lambda x: basic_var
fstring_ref = f"value: {basic_var}"
boolop_ref = basic_var and 1
compare_ref = basic_var > 0
listcomp_ref = [basic_var for _ in []]
dictcomp_ref = {basic_var: 1 for _ in []}
list_ref = [basic_var, 1]
tuple_ref = (basic_var, 1)
set_ref = {basic_var}
dict_ref = {1: basic_var}
call_ref = print(basic_var)
binop_ref = basic_var + 1
lambda_scope = lambda basic_var: basic_var

def annotated_param_func(field_names: list | None = None) -> None:
    if field_names is None or 'a' in field_names or 'b' in field_names:
        return
    return field_names

class DisplayNameRelatedModel(models.Model):
    _name = "pygls.tests.display_name_related_model"
    _description = "Display Name Related Model"

    partner_id = fields.Many2one("res.partner")
    partner_display_name = fields.Char(related="partner_id.display_name", store=True)
    partner_create_uid = fields.Many2one(related="partner_id.create_uid", store=True)
