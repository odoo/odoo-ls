from odoo import models
from odoo.addons.base.models.ir_asset import AssetPaths


class InstanceAttrProbe(models.Model):
    _name = "module_1.instance_attr_probe"
    _description = "Instance Attr Probe"

    def probe(self, paths: AssetPaths):
        paths.memo
        self.cache = {}

    def other(self):
        return self.cache

    def get_class(self):
        InstanceAttrProbe.__class__
        return self.__class__
