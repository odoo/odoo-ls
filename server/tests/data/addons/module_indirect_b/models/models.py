from odoo import models


class IndirectModelB(models.Model):
    _inherit = "ols.indirect.model"

    def own_method(self):
        return False

    def caller_method(self):
        self.own_method()
        return self.indirect_method()

    def get_record(self):
        return self

    def duplicated_base_method(self):
        # get_record has two evaluations (one per override), leading to the same class
        return self.get_record().indirect_method()
