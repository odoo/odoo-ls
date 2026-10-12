from odoo import models
from odoo.exceptions import UserError


class PylintUnlink(models.Model):
    _name = 'bike_parts.pylint_unlink'
    _description = 'Pylint Unlink'

    def unlink(self):
        if not self:
            raise UserError("Nothing to delete")  # expect: pylint_E8503
        return super().unlink()


class NotAModel:
    def unlink(self):
        raise ValueError("Not an Odoo model")
