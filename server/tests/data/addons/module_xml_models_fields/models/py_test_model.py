from odoo import api, fields, models, _, Command

class DemoModel(models.Model):
    _name = 'x_test_model' # OLS03303

    def method(self):
        self.env["x_test_model_m2o"].search([("x_other_model.x_name","=",self.name)])

    def record_values(self):
        # The values of a model defined in XML only: its XML fields and the fields of BaseModel
        self.env["x_test_model_m2o"].create({"x_name": "x", "x_other_model": False, "display_name": "x"})
        self.env["x_test_model_m2o"].write({"x_unknown_field": "x"}) # OLS03014
        # The fields of the delegated model are fields of the delegating model
        self.env["x_delegating_model"].create({"own_field": "x", "parent_only_field": "x", "parent_id": False})
        self.env["x_delegating_model"].write({"x_unknown_field": "x"}) # OLS03014

