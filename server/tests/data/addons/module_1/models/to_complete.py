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


class CallArgumentsModel(models.Model):
    _name = "module_1.call_arguments_model"
    _description = "Call Arguments Model"

    name = fields.Char()
    name_count = fields.Integer()
    parent_id = fields.Many2one("module_1.call_arguments_model")
    # A model name, by position or by keyword
    lambda_id = fields.Many2one("module_1.lambda_def")
    other_lambda_id = fields.Many2one(comodel_name="module_1.lambda_def")
    # A field of the comodel, by position or by keyword
    child_ids = fields.One2many("module_1.call_arguments_model", "par")
    other_child_ids = fields.One2many("module_1.call_arguments_model", inverse_name="par")
    # A field path ending on a field of the same type
    parent_name = fields.Char(related="parent_id.na")

    def search_by_domain(self):
        # A domain, by position or by keyword
        self.search([("na", "=", "x")])
        return self.search(domain=[("na", "=", "x")])


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


class OrderSearchModel(models.Model):
    _inherit = "module_1.order_keywords_model"

    def search_by_order(self):
        # An order given to a search method, by keyword or by position, on the model searched
        self.search([], order="name d")
        self.search([], 0, None, "escape_id.")
        self.env["module_1.order_escape_model"].search_read([], order="")
        # The order of a method that is not a search method
        return self.with_context(order="na")


class OrderSearchOverrideModel(models.Model):
    _name = "module_1.order_search_override_model"
    _description = "Order Search Override Model"

    name = fields.Char()

    def _search(self, domain, offset=0, limit=None, *args, **kwargs):
        # An override that takes the order in `*args`, or in `**kwargs` by keyword
        return super()._search(domain, offset, limit, *args, **kwargs)

    def search_read(self, domain=None, fields=None, offset=0, limit=None, order="id", **read_kwargs):
        # An override that declares `order` too, with a default order
        return super().search_read(domain, fields, offset, limit, order, **read_kwargs)

    def search_by_order_override(self):
        self._search([], 0, None, "na")
        self._search([], order="na")
        self.search_read([], None, 0, None, "na")
