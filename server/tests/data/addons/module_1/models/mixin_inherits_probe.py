from odoo import fields, models


class InheritsTargetProbe(models.Model):
    _name = "module_1.inherits_target_probe"
    _description = "Inherits Target Probe"

    target_value = fields.Integer()


class InheritsMixinProbe(models.AbstractModel):
    _name = "module_1.inherits_mixin_probe"
    _description = "Inherits Mixin Probe"
    _inherits = {"module_1.inherits_target_probe": "target_id"}

    target_id = fields.Many2one("module_1.inherits_target_probe")


class InheritsConcreteProbe(models.Model):
    _name = "module_1.inherits_concrete_probe"
    _inherit = ["module_1.inherits_mixin_probe"]
    _description = "Inherits Concrete Probe"

    def use_target_value(self):
        self.target_value = 1
        return self.target_value
