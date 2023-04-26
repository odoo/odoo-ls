import ast
import os
from .constants import *
from .odoo import *
from .pythonOdooBuilderBase import PythonOdooBuilder


class PythonOdooBuilderV14(PythonOdooBuilder):

    def _load_class_inherit(self, symbol):
        _inherit = symbol.get_class_symbol("_inherit", prevent_comodel=True)
        if _inherit and _inherit.eval.getSymbol():
            inherit_value, _ = _inherit.eval.getSymbol().follow_ref()
            if inherit_value.type == SymType.PRIMITIVE:
                inherit_names = inherit_value.eval.value
                if isinstance(inherit_names, str):
                    symbol.modelData.inherit = [inherit_names]
                elif isinstance(inherit_names, list):
                    symbol.modelData.inherit = inherit_names
                else:
                    print("wrong inherit")
            else:
                print("wrong inherit")

    def _evaluate_name(self, symbol):
        _name = symbol.get_class_symbol("_name", prevent_comodel=True)
        if _name:
            if _name.eval and _name.eval.getSymbol():
                name_value, _ = _name.eval.getSymbol().follow_ref()
                if name_value.type == SymType.PRIMITIVE and name_value.eval.value:
                    return name_value.eval.value
            else:
                return None
        inherit_names = symbol.modelData.inherit
        if len(inherit_names) == 1:
            return inherit_names[0]
        return symbol.name

    def _load_class_name(self, symbol):
        symbol.modelData.name = self._evaluate_name(symbol)
        if not symbol.modelData.name:
            symbol.modelData = None
            return
        if symbol.modelData.name != 'base':
            symbol.modelData.inherit.append('base')

    def _load_class_inherits(self, symbol):
        _inherits = symbol.get_class_symbol("_inherits", prevent_comodel=True)
        if _inherits:
            inherit_value, instance = _inherits.eval.getSymbol().follow_ref()
            if inherit_value.type == SymType.PRIMITIVE:
                inherit_names = inherit_value.eval.value
                if isinstance(inherit_names, dict):
                    symbol.modelData.inherits = inherit_names
                else:
                    print("wrong inherits")
            elif inherit_value.name != "frozendict" or not instance:
                print("wrong inherits")