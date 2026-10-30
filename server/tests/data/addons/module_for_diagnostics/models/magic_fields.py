# Magic fields of the models, asserted by tests/diagnostics/magic_fields_comodel.rs: one case per
# call, found by its line.
from odoo import fields, models


class MagicFieldsRecord(models.Model):
    _name = 'magic_fields.record'
    _description = 'Magic Fields Record'

    name = fields.Char()

    def magic_domains(self):
        # `create_uid` and `write_uid` are many2one to `res.users`
        self.search([('create_uid.login', '=', 'x')])
        self.search([('write_uid.partner_id.name', '=', 'x')])
        self.search([('create_uid.unknown_user_field', '=', 'x')])
