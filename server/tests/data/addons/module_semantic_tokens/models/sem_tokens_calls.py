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

    def _search_with_order(self):
        # An order given to a search method, by keyword or by position, names fields of the model searched
        self.search([], order='other_id desc, name')
        self.search_fetch([], ['total'], 0, None, 'other_id')
        self.env['sem.tokens.other'].search_read([], order='other_name')
        # The order of a method that is not a search method names nothing
        return self.with_context(order='mirrored')


class SemTokensSearchOverride(models.Model):
    _inherit = 'sem.tokens.usage'

    def _search(self, domain, *args, **kwargs):
        # An override that does not declare `order`: the base method does
        return super()._search(domain, *args, **kwargs)

    def _search_override_orders(self):
        self._search([], order='other_id')
        return self._search([], 0, None, 'name')
