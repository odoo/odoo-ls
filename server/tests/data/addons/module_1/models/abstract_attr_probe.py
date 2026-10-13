from odoo import models


class AbstractAttrProbe(models.AbstractModel):
    _name = "module_1.abstract_attr_probe"
    _description = "Abstract Attr Probe"

    def set_value(self):
        # a mixin can use fields declared by the models inheriting from it
        self.concrete_value = 1
