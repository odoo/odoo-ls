from odoo import models


class IndirectModelB(models.Model):
    _inherit = "ols.indirect.model"

    def own_method(self):
        return False

    def caller_method(self):
        self.own_method()
        return self.indirect_method()
