# 1. Imports are resolved
from odoo import api, fields, models

class BikePartsWheel(models.Model):
    _name = 'bike_parts.wheel'
    _description = 'Bike Wheel'

    name = fields.Char(string='Wheel Name', required=True)
    price = fields.Float(string='Price', required=True)
    available = fields.Boolean(string='Available', default=True)
    description = fields.Text(string='Description')

    def action_set_available(self):
        self.available = True

class BikesBike(models.Model):
    _name = 'bikes.bike'

    name = fields.Char(string='Wheel Name', required=True, translate=True)
    wheel_id = fields.Many2one('bike_parts.wheel', string='Wheel')
    wheel_ids = fields.Many2many('bike_parts.wheel', string='Wheels')
    restricted = fields.Char(groups='base.group_user,!base.group_portal,module_for_diagnostics.no_such_group') # OLS05054
    hidden = fields.Char(groups='.') # Ok, odoo.fields.NO_ACCESS
    not_a_group = fields.Char(groups='module_for_diagnostics.bike_1') # OLS05054
    bike_weight = fields.Float(string='Bike Weight (kg)', compute='_compute_bike_weight', store=True)

    @api.depends('wheel_id.price')
    def _compute_bike_weight(self):
        for bike in self:
            if bike.wheel_id:
                bike.bike_weight = bike.wheel_id.price * 0.5
            else:
                bike.bike_weight = 0.0
        self.env.ref('module_for_diagnostics.bike_wheel_DOES_NOT_EXIST') # OLS05001
        self.env.ref('bike_wheel_DOES_NOT_EXIST') # OLS05002
        self.env.ref('module_for_diagnostics.bike_wheel_6') # Ok
        self.env.ref('base.module_base') # Ok, the registry generates it
        self.env.ref('base.field_res_partner__name') # Ok, the registry generates it
        self.env.ref('base.selection__res_partner__type__contact') # Ok, the registry generates it
        self.env.ref('WRONG_MODULE.bike_wheel_6') # OLS05003
        self.env.ref('module_for_diagnostics.bike_wheel_6.too.many.dots') # OLS05051
        self.env.ref('WRONG_MODULE.bike_wheel_6', False) # Ok, the caller takes an empty result
        self.env.ref('WRONG_MODULE.bike_wheel_6', raise_if_not_found=False) # Ok, same spelled by name
