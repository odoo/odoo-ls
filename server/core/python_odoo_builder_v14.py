import ast
import os
from ..constants import *
from .odoo import *
from .python_odoo_builder_base import PythonOdooBuilder


class AttributeNotFound():
    pass


class PythonOdooBuilderV14(PythonOdooBuilder):

    def _load_class_inherit(self, symbol):
        _inherit = symbol.get_symbol([], ["_inherit"])
        if _inherit and _inherit.eval and _inherit.eval.get_symbol():
            inherit_value, _ = _inherit.eval.get_symbol().follow_ref()
            if inherit_value.type == SymType.PRIMITIVE:
                inherit_names = inherit_value.value
                if isinstance(inherit_names, str):
                    symbol.modelData.inherit = [inherit_names]
                elif isinstance(inherit_names, list):
                    symbol.modelData.inherit = inherit_names[:]
            else:
                print("wrong inherit")

    def _evaluate_name(self, symbol):
        _name = symbol.get_symbol([], ["_name"])
        if _name:
            if _name.eval and _name.eval.get_symbol():
                name_value, _ = _name.eval.get_symbol().follow_ref()
                if name_value.type == SymType.PRIMITIVE and name_value.value:
                    return name_value.value
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
        _inherits = symbol.get_symbol([], ["_inherits"])
        if _inherits and _inherits.eval.get_symbol():
            inherit_value, instance = _inherits.eval.get_symbol().follow_ref()
            if inherit_value.type == SymType.PRIMITIVE:
                inherit_names = inherit_value.value
                if isinstance(inherit_names, dict):
                    symbol.modelData.inherits = inherit_names
                else:
                    print("wrong inherits")
            elif inherit_value.name != "frozendict" or instance:
                print("wrong inherits")

    def _get_attribute(self, symbol, attr):
        attr_sym = symbol.get_member_symbol(attr, prevent_comodel=True)
        if attr_sym and attr_sym.eval.get_symbol():
            attr_ref, instance = attr_sym.eval.get_symbol().follow_ref()
            if attr_ref.type == SymType.PRIMITIVE:
                attr_value = attr_ref.value
                return attr_value
        return AttributeNotFound

    def _load_class_attributes(self, symbol):
        #TODO this doesn't make this symbols available in the class... So attributes can't be called
        symbol.modelData.description = self._get_attribute(symbol, "_description")
        if symbol.modelData.description == AttributeNotFound or symbol.modelData.description == None: #should not happen, as auto is defined on BaseModel
            symbol.modelData.description = symbol.modelData.name
        symbol.modelData.auto = self._get_attribute(symbol, "_auto")
        if symbol.modelData.auto == AttributeNotFound: #should not happen, as auto is defined on BaseModel
            symbol.modelData.auto = False
        symbol.modelData.log_access = self._get_attribute(symbol, "_log_access")
        if symbol.modelData.log_access == AttributeNotFound:
            symbol.modelData.log_access = symbol.modelData.auto
        symbol.modelData.table = self._get_attribute(symbol, "_table")
        if symbol.modelData.table == AttributeNotFound or symbol.modelData.table == None:
            symbol.modelData.table = symbol.modelData.name.replace(".", "_")
        symbol.modelData.sequence = self._get_attribute(symbol, "_sequence")
        if symbol.modelData.sequence == AttributeNotFound or symbol.modelData.sequence == None:
            symbol.modelData.sequence = symbol.modelData.table + "_id_seq"
        symbol.modelData.abstract = self._get_attribute(symbol, "_abstract")
        if symbol.modelData.abstract == AttributeNotFound:
            symbol.modelData.abstract = True
        symbol.modelData.transient = self._get_attribute(symbol, "_transient")
        if symbol.modelData.transient == AttributeNotFound:
            symbol.modelData.transient = False
        #TODO check that rec_name is pointing to a valid field in the model
        symbol.modelData.rec_name = self._get_attribute(symbol, "_rec_name")
        if symbol.modelData.rec_name == AttributeNotFound or symbol.modelData.rec_name == None:
            symbol.modelData.rec_name = 'name' #TODO if name is not in the model, take 'id'
        symbol.modelData.check_company_auto = self._get_attribute(symbol, "_check_company_auto")
        if symbol.modelData.check_company_auto == AttributeNotFound:
            symbol.modelData.check_company_auto = False
        symbol.modelData.parent_name = self._get_attribute(symbol, "_parent_name")
        if symbol.modelData.parent_name == AttributeNotFound:
            symbol.modelData.parent_name = 'parent_id'
        symbol.modelData.parent_store = self._get_attribute(symbol, "_parent_store")
        if symbol.modelData.parent_store == AttributeNotFound:
            symbol.modelData.parent_store = False
        symbol.modelData.active_name = self._get_attribute(symbol, "_active_name")
        if symbol.modelData.active_name == AttributeNotFound:
            symbol.modelData.active_name = None
        symbol.modelData.date_name = self._get_attribute(symbol, "_date_name")
        if symbol.modelData.date_name == AttributeNotFound:
            symbol.modelData.date_name = 'date'
        symbol.modelData.fold_name = self._get_attribute(symbol, "_fold_name")
        if symbol.modelData.fold_name == AttributeNotFound:
            symbol.modelData.fold_name = 'fold'
