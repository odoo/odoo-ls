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

    def find_orders_for_user(self):
        return self.search([('shop_id.manager_id', 'access', 'read')])
