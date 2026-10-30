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


class SemTokensSearchDomains(models.Model):
    _inherit = 'sem.tokens.usage'

    def _search_other_domains(self):
        # The domain of the other search methods names fields of the model searched, by position or by keyword
        self.search_count([('other_id.other_name', '=', False)])
        self.search_fetch([('other_id', '=', False)], ['name'])
        return self.search_read(domain=[('other_id.other_name', '=', False)])

    def _other_domains(self):
        # The domain of the other methods taking one names fields of the model they are called on
        self.filtered_domain([('other_id.other_name', '=', False)])
        self._read_group([('other_id', '=', False)], ['name'])
        return self.name_search('x', [('other_id.other_name', '=', False)])
