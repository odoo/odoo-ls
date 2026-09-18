from odoo import fields, models


class BikeComputeSql(models.Model):
    _name = 'bike_parts.compute_sql'
    _description = 'Bike Compute Sql'

    part_count = fields.Integer(compute_sql='_compute_part_count')
    missing_count = fields.Integer(compute_sql='_compute_missing_count')
    missing_init = fields.Integer(init_storage='_init_missing')

    def _compute_part_count(self):
        pass
