# Values of `create`, `write` and `update`, asserted by tests/diagnostics/ols03014_record_values.rs: one
# case per call, found by its line, which must stay unique and ASCII-only.
from odoo import fields, models


class RecordValuesRecord(models.Model):
    _name = 'record_values.record'
    _description = 'Record Values Record'

    name = fields.Char()
    parent_id = fields.Many2one('record_values.record')
    other_ids = fields.One2many('record_values.other', 'record_id')

    def record_values(self, names, other_values):
        self.create({'name': 'x', 'parent_id': False})
        self.create({'unknown_create_field': 'x'})
        self.create([{'name': 'x'}, {'unknown_create_list_field': 'x'}])
        self.create([{'unknown_create_comp_field': name} for name in names])
        self.write({'unknown_write_field': 'x'})
        self.update({'unknown_update_field': 'x'})
        # The magic fields are fields of every model
        self.write({'write_date': False, 'create_uid': False})
        # Only the string keys are checked: not the values, a spread or a computed key
        self.write({'name': 'unknown_value_field', **other_values, names[0]: 'x'})
        # The fields of the model the method is called on, not of the class the call is in
        self.env['record_values.other'].create({'parent_id': False})
        # The values of a x2many command are not checked against the model the method is called on
        self.create({'other_ids': [(0, 0, {'other_name': 'x'})]})
        # The values given by keyword
        self.create(vals_list=[{'name': 'x'}, {'unknown_create_keyword_field': 'x'}])
        self.write(vals={'unknown_write_keyword_field': 'x'})
        self.update(values={'unknown_update_keyword_field': 'x'})
        # `write` takes no list of values
        self.write([{'unknown_write_list_field': 'x'}])
        # Not a model's update
        return {}.update({'unknown_dict_field': 'x'})


class RecordValuesOther(models.Model):
    _name = 'record_values.other'
    _description = 'Record Values Other'

    other_name = fields.Char()
    record_id = fields.Many2one('record_values.record')

    def write(self, vals):
        # The values given to `super()` are on the model of the class
        return super().write({**vals, 'other_name': 'x', 'unknown_super_field': 'x'})
