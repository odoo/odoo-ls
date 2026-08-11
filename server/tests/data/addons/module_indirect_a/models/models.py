from odoo import models


class IndirectModelA(models.Model):
    _inherit = "ols.indirect.model"

    def indirect_method(self):
        return True
