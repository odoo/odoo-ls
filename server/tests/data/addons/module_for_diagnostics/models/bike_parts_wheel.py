# 1. Imports are resolved
from odoo import api, fields, models

class BikePartsWheel(models.Model):
    _name = 'bike_parts.wheel'
    _description = 'Bike Wheel'

    name = fields.Char(string='Wheel Name', required=True)
    price = fields.Float(string='Price', required=True)
    available = fields.Boolean(string='Available', default=True)
    description = fields.Text(string='Description')

class BikesBike(models.Model):
    _name = 'bikes.bike'

    name = fields.Char(string='Wheel Name', required=True)
    wheel_id = fields.Many2one('bike_parts.wheel', string='Wheel')
    bike_weight = fields.Float(string='Bike Weight (kg)', compute='_compute_bike_weight', store=True)

    @api.depends('wheel_id.price')
    def _compute_bike_weight(self):
        for bike in self:
            if bike.wheel_id:
                bike.bike_weight = bike.wheel_id.price * 0.5
            else:
                bike.bike_weight = 0.0
        self.env.ref('module_for_diagnostics.bike_wheel_DOES_NOT_EXIST') # TODO: OLS05001
        self.env.ref('bike_wheel_DOES_NOT_EXIST') # OLS05002
        self.env.ref('module_for_diagnostics.bike_wheel_6') # Ok
        self.env.ref('WRONG_MODULE.bike_wheel_6') # OLS05003
