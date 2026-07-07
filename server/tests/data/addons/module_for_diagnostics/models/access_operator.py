from odoo import fields, models


class BikeShop(models.Model):
    _name = 'bike_parts.shop'
    _description = 'Bike Shop'

    name = fields.Char(string='Shop Name', required=True)
    manager_id = fields.Many2one('res.users', string='Manager')


class BikeOrder(models.Model):
    _name = 'bike_parts.order'
    _description = 'Bike Order'

    name = fields.Char(string='Reference', required=True)
    shop_id = fields.Many2one('bike_parts.shop', string='Shop')
    ref_code = fields.Many2one('bike_parts.shop', string='Ref Code')

    def find_orders_for_user(self):
        return self.search([('shop_id.manager_id', 'access', 'read')])

    def find_orders_invalid_field(self):
        return self.search([('name', 'access', 'read')])

    def find_orders_invalid_value(self):
        return self.search([('shop_id', 'access', 'delete')])


class BikeOrderInherit(models.Model):
    _inherit = 'bike_parts.order'

    # redeclared here so the field resolves to more than one candidate symbol,
    # exercising the multi-candidate path of the access-field check
    name = fields.Char(string='Reference', required=True, help='Overridden in another module')

    def find_orders_invalid_field_multi_candidate(self):
        return self.search([('name', 'access', 'read')])
