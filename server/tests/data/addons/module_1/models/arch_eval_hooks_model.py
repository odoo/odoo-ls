from odoo import api, fields, models, _, tools
from odoo import SUPERUSER_ID, Command, _lt


class ArchEvalHooksModel(models.Model):
    _name = "pygls.tests.arch_eval_hooks_model"
    _description = "Model dedicated to exercising python_arch_eval_hooks.rs hooks not covered elsewhere"

    test_int = fields.Integer()

    def chain_methods(self):
        sudo_res = self.sudo()
        with_env_res = self.with_env(self.env)
        with_company_res = self.with_company(1)
        with_context_res = self.with_context(lang="en_US")
        with_prefetch_res = self.with_prefetch()
        filtered_res = self.filtered(lambda r: r.test_int)
        filtered_domain_res = self.filtered_domain([("test_int", ">", 0)])
        exists_res = self.exists()
        browse_res = self.browse([1])
        with_user_res = self.with_user(1)
        create = self.create({"test_int": 1})
        search = self.search([("test_int", ">", 0)])
        return sudo_res, with_env_res, with_company_res, with_context_res, with_prefetch_res, filtered_res, filtered_domain_res, exists_res, browse_res

    def registry_methods(self):
        registry_getitem_res = self.env.registry["res.partner"]
        registry_prop_res = self.env.registry
        return registry_getitem_res, registry_prop_res

    def misc_fields(self):
        ids_res = self.ids
        id_res = self.id
        return ids_res, id_res

    def odoo_init_symbols(self):
        superuser_id_res = SUPERUSER_ID
        command_res = Command
        lt_res = _lt("test")
        underscore_res = _("test")
        return superuser_id_res, command_res, lt_res, underscore_res

    def ir_rule_global(self):
        self.env["ir.rule"].search([("global", "=", True)])

    # --- Fields exercising the scalar/relational Field.__get__ hooks that
    # are not already covered by other tests. ---
    test_boolean = fields.Boolean()
    test_float = fields.Float()
    currency_id = fields.Many2one("res.currency")
    test_monetary = fields.Monetary()
    test_char = fields.Char()
    test_text = fields.Text()
    test_html = fields.Html()
    test_date = fields.Date()
    test_datetime = fields.Datetime()
    test_binary = fields.Binary()
    test_image = fields.Image()
    test_selection = fields.Selection([("a", "A"), ("b", "B")])
    test_reference = fields.Reference(selection=[("res.partner", "Partner")])
    test_json = fields.Json()
    test_properties_definition = fields.PropertiesDefinition()
    test_properties = fields.Properties(definition="parent_id.test_properties_definition")
    parent_id = fields.Many2one("pygls.tests.arch_eval_hooks_model")
    child_ids = fields.One2many("pygls.tests.arch_eval_hooks_model", "parent_id")
    partner_ids = fields.Many2many("res.partner")

    def env_hook(self):
        env_res = self.env
        return env_res

    def scalar_field_types(self):
        boolean_res = self.test_boolean
        float_res = self.test_float
        monetary_res = self.test_monetary
        char_res = self.test_char
        text_res = self.test_text
        html_res = self.test_html
        date_res = self.test_date
        datetime_res = self.test_datetime
        binary_res = self.test_binary
        image_res = self.test_image
        selection_res = self.test_selection
        reference_res = self.test_reference
        json_res = self.test_json
        properties_res = self.test_properties
        properties_definition_res = self.test_properties_definition
        return (boolean_res, float_res, monetary_res, char_res, text_res, html_res, date_res,
                datetime_res, binary_res, image_res, selection_res, reference_res, json_res,
                properties_res, properties_definition_res)

    def relational_field_types(self):
        child_ids_res = self.child_ids
        partner_ids_res = self.partner_ids
        return child_ids_res, partner_ids_res
