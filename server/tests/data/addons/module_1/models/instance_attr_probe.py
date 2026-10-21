from odoo import fields, models
from odoo.addons.base.models.ir_asset import AssetPaths


class InstanceAttrProbe(models.Model):
    _name = "module_1.instance_attr_probe"
    _description = "Instance Attr Probe"

    def probe(self, paths: AssetPaths):
        paths.memo
        self.cache = {}

    def other(self):
        return self.cache

    def set_field(self):
        self.value = 1

    value = fields.Integer()

    def pick(self, flag):
        if flag:
            return self.env['module_1.instance_attr_probe']
        return self.env['res.partner']

    def use_pick(self):
        self.pick(True).value
        self.pick(True).not_on_any_candidate
