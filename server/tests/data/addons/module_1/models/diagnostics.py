from odoo import api, fields, models, _, tools
from odoo.addons.module_2.models import base_test_models # OLS03003

class ModelWithDiagnostics(models.Model):
    _name = "module_1.diagnostics_model"
    _inherit = "module_2.empty_model" # OLS03004
    _description = "Model to test diagnostics"

    int_field = fields.Integer()
    test_models = fields.One2many("pygls.tests.base_test_model", "diagnostics_id")
    test_models_wrong_dep = fields.One2many("module_2.custom_model", "diag_id") # OLS03015, OLS03021
    test_models_wrong_dep_kw = fields.One2many(comodel_name="module_2.custom_model", inverse_name="diag_id") # OLS03015, OLS03021
    test_no_model = fields.Many2one("non.existent.model") # OLS03016
    test_no_model_kw = fields.Many2one(comodel_name="non.existent.model") # OLS03016
    test_related = fields.Integer(related="test_models.test_int")
    test_related_wrong = fields.Integer(related="test_models.diagnostics_id") #OLS03017

    test_models = fields.One2many("pygls.tests.base_test_model", "test_int_vdsqcedfc") # OLS03021
    test_models = fields.One2many("pygls.tests.base_test_model", "test_int") # OLS03022
    test_models = fields.One2many("pygls.tests.base_test_model", inverse_name="test_int_vdsqcedfc") # OLS03021
    test_models = fields.One2many("pygls.tests.base_test_model", inverse_name="test_int") # OLS03022

    test_models = fields.One2many("pygls.tests.base_test_model", inverse_name="same_name_id") # OLS03023


    test_method_search_1 = fields.Integer(search="_search_1")
    test_method_search_2 = fields.Integer(search="_search_2") # OLS03018
    test_method_search_3 = fields.Integer(compute="_compute_1")
    test_method_search_4 = fields.Integer(compute="_compute_2") # OLS03018
    test_method_search_5 = fields.Integer(inverse="_inverse_1")
    test_method_search_6 = fields.Integer(inverse="_inverse_2") # OLS03018

    date = fields.Date()

    to_compute = fields.Integer(compute="_compute_field")

    def a_method(self):
        self.env["module_2.empty_model"] # OLS03001
        self.env["non.existent.model"] # OLS03002
        self.env["module_1.no_base_model"] # OLS03005

    def test_search_domain(self):
        self.search(5) # OLS03006
        self.search(("int_field", "=", 0)) # OLS03006
        self.search([("int_field", "=", 0)])
        self.search([("int_field", "!=", 0)])
        self.search([("int_field", ">", 0)])
        self.search([("int_field", "<", 0)])
        self.search([("int_field", ">=", 0)])
        self.search([("int_field", "<=", 0)])
        self.search([("int_field", "like", 0)])
        self.search([("int_field", "ilike", 0)])
        self.search([("int_field", "in", [0])])
        self.search([("int_field", "not in", [0])])
        self.search([("test_models", "child_of", 0)])
        self.search([("test_models", "parent_of", 0)])
        self.search([("test_models", "any", [])])
        self.search([("test_models", "not any", [])])
        a = [("int_field", "=", 0)]
        self.search(a)
        self.search([])
        self.search([("int_field",)]) # OLS03007
        self.search([("int_field", "=")]) # OLS03007
        self.search([("|", "int_field", "=", 0)]) # OLS03007
        self.search(["|", ("int_field", "=", 0), ("int_field", "=", 1)])
        self.search(["&", ("int_field", "=", 0), ("int_field", "=", 1)])
        self.search(["!", ("int_field", "=", 0)])
        self.search(["or", ("int_field", "=", 0), ("int_field", "=", 1)]) # OLS03008
        self.search(["not", ("int_field", "=", 0)]) # OLS03008
        self.search([("int_field", "lt", 0)]) # OLS03009
        self.search(["|", ("int_field", "=", 0)]) # OLS03010
        self.search(["&", ("int_field", "=", 0)]) # OLS03010
        self.search(["!"]) # OLS03010
        self.search(["!", ("int_field", "=", 0), ("int_field", "=", 0)])
        self.search([("wrong_field", "=", 0)]) # OLS03011
        self.search([("test_models.partner_id", "=", 0)])
        self.search([("test_models.wrong_field", "=", 0)]) # OLS03011

        self.search([("date.year_number", "=", 0)])
        self.search([("date.quarter_number", "=", 0)])
        self.search([("date.month_number", "=", 0)])
        self.search([("date.iso_week_number", "=", 0)])
        self.search([("date.day_of_week", "=", 0)])
        self.search([("date.day_of_month", "=", 0)])
        self.search([("date.day_of_year", "=", 0)])
        self.search([("date.hour_number", "=", 0)])
        self.search([("date.minute_number", "=", 0)])
        self.search([("date.second_number", "=", 0)])
        self.search([("date.millisecond_number", "=", 0)]) # OLS03012

        self.search([("int_field.wrong_attr", "=", 0)]) # OLS03013
        self.search([("test_models.wrong_attr", "=", 0)]) # OLS03011

    @api.depends("int_field")
    @api.depends("wrong_field") # OLS03014
    @api.onchange("int_field")
    @api.onchange("wrong_field") # OLS03014
    @api.constrains("int_field")
    @api.constrains("wrong_field") # OLS03014
    def _compute_field(self):
        pass

    def _search_1(self, operator, value):
        pass

    def _compute_1(self):
        self.test_method_search_3 = 5
        self.test_method_search_4 = 10 # OLS03019

    def _inverse_1(self):
        pass

class SameNameModel(models.Model):
    _name = "module_1.same_name_model" # OLS03020
    _description = "Model with same name as another model"

class SameNameModel2(models.Model):
    _name = "module_1.same_name_model" # OLS03020
    _description = "Another model with same name"