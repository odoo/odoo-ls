# Strings in calls: only the arguments of a call take its context, its callee keeps the one around
# the call. Same rules as sem_tokens_main.py: ASCII-only, asserted identifiers unique on their line.
from odoo import fields, models


class SemTokensCalls(models.Model):
    _inherit = 'sem.tokens.usage'

    # The path is in the callee of `strip()`, inside the `related` argument of `fields.Char` but not its value
    stripped = fields.Char(related='other_id.other_name'.strip())

    def _search_by_keyword(self):
        # A domain given by keyword, as by position
        return self.search(domain=[('other_id.other_name', '=', False)])
