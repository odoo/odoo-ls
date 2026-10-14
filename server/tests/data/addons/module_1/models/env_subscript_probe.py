from odoo import models


class EnvSubscriptProbe(models.Model):
    _name = "module_1.env_subscript_probe"
    _description = "Env Subscript Probe"

    def make_partner(self):
        return self.env["res.partner"].create({})
