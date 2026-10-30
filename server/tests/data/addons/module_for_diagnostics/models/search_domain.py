# `domain` of the search methods, asserted by tests/diagnostics/ols03011_search_methods.rs: one case per
# call, found by its line, which must stay unique and ASCII-only.
from odoo import fields, models


class SearchDomainRecord(models.Model):
    _name = 'search_domain.record'
    _description = 'Search Domain Record'

    name = fields.Char()
    parent_id = fields.Many2one('search_domain.record')

    def search_domains(self):
        self.search_count([('parent_id.name', '=', 'x')])
        self.search_count([('unknown_count_field', '=', 'x')])
        self.search_fetch([('unknown_fetch_field', '=', 'x')], ['name'])
        self.search_read([('unknown_read_field', '=', 'x')])
        self._search([('unknown_private_field', '=', 'x')])
        # By keyword
        self.search_count(domain=[('unknown_count_kwarg_field', '=', 'x')])
        self.search_read(domain=[('unknown_read_kwarg_field', '=', 'x')])
        # The fields of the model searched, not of the class the call is in
        self.env['search_domain.kwargs'].search_count([('parent_id', '=', False)])
        # Not a search method
        return self.with_context(domain=[('unknown_context_field', '=', 'x')])

    def other_domains(self):
        # The other methods taking a domain
        self.filtered_domain([('unknown_filtered_field', '=', 'x')])
        self._read_group([('unknown_private_group_field', '=', 'x')], ['name'])
        self.name_search('x', [('unknown_name_search_field', '=', 'x')])


class SearchDomainKwargs(models.Model):
    _name = 'search_domain.kwargs'
    _description = 'Search Domain Kwargs'

    name = fields.Char()

    def _search(self, *args, **kwargs):
        # An override that does not declare `domain`: it goes to `*args`, or to `**kwargs` by keyword
        return super()._search(*args, **kwargs)

    def kwargs_domains(self):
        self._search([('name', '=', 'x')])
        self._search([('unknown_args_field', '=', 'x')])
        self._search(domain=[('unknown_kwargs_field', '=', 'x')])


class SearchDomainDeclared(models.Model):
    _name = 'search_domain.declared'
    _description = 'Search Domain Declared'

    name = fields.Char()

    def search_count(self, domain, limit=None):
        # An override that declares `domain` too, as many in Odoo do
        return super().search_count(domain, limit)

    def declared_domains(self):
        self.search_count([('unknown_declared_field', '=', 'x')])
        self.search_count(domain=[('unknown_declared_kwarg_field', '=', 'x')])
