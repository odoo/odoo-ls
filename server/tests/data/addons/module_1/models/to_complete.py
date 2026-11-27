from odoo import api, fields, models, _, tools


class TestModel(models.Model):

    pass

ExtraTestModel = TestModel
SuperExtraTestModel = ExtraTestModel
testModel = TestModel()
extraTestModel = ExtraTestModel()
superExtraTestModel = SuperExtraTestModel()

class LambdaDefaultModel(models.Model):
    _name = "module_1.lambda_default_model"
    _description = "Lambda Default Model"

    company_id = fields.Many2one(
        'res.company', 'Company',
        default=lambda self: self.env.company,
        index=True,
    )

    def _use_company(self):
        return self

    def _use_company(self):
        import odoo
        return odoo.Comm

    def _use_creator(self):
        # `create_uid` is a many2one to `res.users`
        return self.create_uid.log

    def _use_company_list(self):
        companies = [self.company_id]
        company_pairs = (self.company_id, self.company_id)
        return companies, company_pairs

    def _search_company(self):
        self.search([("company_id", "=", 1)])
