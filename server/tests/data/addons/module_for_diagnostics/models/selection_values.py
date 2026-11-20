# Values of the Selection fields given to `create`, `write` and `update`, asserted by
# tests/diagnostics/ols03030_selection_values.rs: one case per call, found by its line, which must stay unique
# and ASCII-only.
from odoo import fields, models

STATES = [('draft', 'Draft'), ('done', 'Done')]


class SelectionValuesRecord(models.Model):
    _name = 'selection_values.record'
    _description = 'Selection Values Record'

    name = fields.Char()
    parent_id = fields.Many2one('selection_values.record')
    state = fields.Selection([('draft', 'Draft'), ('done', 'Done')])
    constant_state = fields.Selection(STATES)
    method_state = fields.Selection('_get_states')
    related_state = fields.Selection(related='parent_id.state')
    added_state = fields.Selection([('draft', 'Draft')])

    def _get_states(self):
        return [('draft', 'Draft')]

    def selection_values(self, value):
        self.write({'state': 'draft'})
        self.write({'state': 'unknown_state'})
        self.create({'constant_state': 'unknown_constant'})
        self.create([{'state': 'done'}, {'state': 'unknown_list'}])
        # Odoo builds the values of a method at runtime
        self.write({'method_state': 'unknown_method'})
        # A related field has the values of its target
        self.write({'related_state': 'done'})
        self.write({'related_state': 'unknown_related'})
        # An extension adds values
        self.write({'added_state': 'extra'})
        self.write({'added_state': 'unknown_added'})
        # Only string values are checked
        self.write({'state': False, 'added_state': value})
        # Not a Selection field
        self.write({'name': 'unknown_name'})


class SelectionValuesExtension(models.Model):
    _inherit = 'selection_values.record'

    added_state = fields.Selection(selection_add=[('extra', 'Extra'), ('draft',)])
