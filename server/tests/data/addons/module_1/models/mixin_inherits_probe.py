from odoo import fields, models


class InheritsDeepProbe(models.Model):
    _name = "module_1.inherits_deep_probe"
    _description = "Inherits Deep Probe"

    deep_value = fields.Integer()


class InheritsTargetProbe(models.Model):
    _name = "module_1.inherits_target_probe"
    _description = "Inherits Target Probe"
    _inherits = {"module_1.inherits_deep_probe": "deep_id"}

    target_value = fields.Integer()
    deep_id = fields.Many2one("module_1.inherits_deep_probe")

    def target_method(self):
        return self.deep_value


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

    def use_deep_value(self):
        return self.deep_value

    def use_target_method(self):
        return self.target_method
