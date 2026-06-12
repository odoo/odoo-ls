from odoo import api, fields, models, _, Command

class DemoModel(models.Model):
    _name = 'x_test_model' # OLS03303

    def method(self):
        self.env["x_test_model_m2o"].search([("x_other_model.x_name","=",self.name)])

