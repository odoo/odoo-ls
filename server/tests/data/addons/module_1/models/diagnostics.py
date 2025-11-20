from odoo import api, fields, models, _, tools
from odoo.addons.module_2.models import base_test_models # OLS03003

class ModelWithDiagnostics(models.Model):
    _name = "module_1.diagnostics_model"
    _inherit = "module_2.empty_model" # OLS03004
    _description = "Model to test diagnostics"

    int_field = fields.Integer()
    test_models = fields.One2many("pygls.tests.base_test_model", "diagnostics_id")
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