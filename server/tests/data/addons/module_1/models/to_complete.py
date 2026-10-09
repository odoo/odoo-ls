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


class OrderEscapeModel(models.Model):
    _name = "module_1.order_escape_model"
    _description = "Order Escape Model"
    _order = "name,\tid"

    name = fields.Char()


class OrderKeywordsModel(models.Model):
    _name = "module_1.order_keywords_model"
    _description = "Order Keywords Model"
    _order = "name desc nulls last, escape_id.id, name.id, id"

    name = fields.Char()
    escape_id = fields.Many2one("module_1.order_escape_model")


class OrderAnnotatedModel(models.Model):
    _inherit = "module_1.order_keywords_model"
    _order: str = "name, "


class OrderPlainClass:
    # Not a model: `_order` is a plain attribute
    _order = "name "


class OrderLocalModel(models.Model):
    _inherit = "module_1.order_keywords_model"

    def local_order(self):
        # A local variable, not the model's `_order`
        _order = "name "
