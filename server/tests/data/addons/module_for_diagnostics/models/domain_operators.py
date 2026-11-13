from odoo import fields, models


class DomainOperators(models.Model):
    _name = 'domain_operators.model'
    _description = 'Domain Operators'

    name = fields.Char()
    parent_id = fields.Many2one('domain_operators.model')

    def search_not_like(self):
        return self.search([('name', 'not =like', 'a%')])

    def search_not_ilike(self):
        return self.search([('name', 'not =ilike', 'a%')])

    def search_internal_any(self):
        return self.search([('parent_id', 'any!', [])])

    def search_internal_not_any(self):
        return self.search([('parent_id', 'not any!', [])])
