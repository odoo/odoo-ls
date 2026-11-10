# `order` of the search methods, asserted by tests/diagnostics/ols03029.rs: one case per call, found by
# its line, which must stay unique and ASCII-only.
from odoo import fields, models


class SearchOrderKwargs(models.Model):
    _name = 'search_order.kwargs'
    _description = 'Search Order Kwargs'

    name = fields.Char()

    def _search(self, domain, *args, **kwargs):
        # An override that does not declare `order`: it goes to `**kwargs`, or to `*args` by position
        return super()._search(domain, *args, **kwargs)

    def kwargs_orders(self):
        self._search([], order='name desc')
        self._search([], order='name dsc')
        self._search([], order='unknown_kwargs_field')
        self._search([], 0, None, 'unknown_args_field')


class SearchOrderRecord(models.Model):
    _name = 'search_order.record'
    _description = 'Search Order Record'

    name = fields.Char()
    parent_id = fields.Many2one('search_order.record')

    def search_orders(self):
        self.search([], order='name desc, parent_id.id')
        self.search([], order='name dsc')
        self.search([], order='unknown_search_field')
        self.search([], 0, None, 'parent_id.name')
        self.search_fetch([], ['name'], order='name nulls')
        self.search_read([], order='name,,id')
        self.search([], order='')
        self.search([], order=self._order)
        # The fields of the model searched, not of the class the call is in
        self.env['search_order.kwargs'].search([], order='parent_id')
        # Not a search method
        return self.with_context(order='unknown_context_field')


class SearchOrderPlainClass:
    # Not a model: its `search` is only named like a search method
    def search(self, query, order=None):
        return query

    def plain_orders(self):
        self.search('x', order='unknown_plain_field desc')


class SearchOrderDeclared(models.Model):
    _name = 'search_order.declared'
    _description = 'Search Order Declared'

    name = fields.Char()

    def search_read(self, domain=None, fields=None, offset=0, limit=None, order=None, **read_kwargs):
        # An override that declares `order` too, as many in Odoo do
        return super().search_read(domain, fields, offset, limit, order, **read_kwargs)

    def declared_orders(self):
        self.search_read([], order='unknown_declared_field')
        self.search_read([], None, 0, None, 'name dsc')
