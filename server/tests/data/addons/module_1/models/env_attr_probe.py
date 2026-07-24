from odoo import models


class EnvAttrProbe(models.Model):
    _name = "module_1.env_attr_probe"
    _description = "Env Attr Probe"

    def probe(self):
        self.env.registry
        self.env.user
        self.env.user.has_group("base.group_user")
        self.env.company
        self.env.companies
        self.env.lang
        user = self.env.user
        user
        user.has_group("base.group_user")
