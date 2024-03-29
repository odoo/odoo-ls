import ast
import os

from .model import Model
from ..constants import *
from .odoo import Odoo
from .symbol import ClassSymbol, ModelData
from .file_mgr import FileMgr


class PythonOdooBuilder(ast.NodeVisitor):

    """The Python Odoo Builder is the step that extracts Odoo models info for the validation.
    It represents data that are loaded and built by Odoo at loading time (model declarations, etc...)
    and that can't be used in a classic linter, due to their dynamic nature.
    This step can't be merged with Arch builder because this construction should be able to be run
    regularly like the validation, but we don't need to reload all symbols, as the file didn't change.
    In the same logic, we can't merge this step with the validation as the validation need to have all
    data coming from the simulated running odoo to work properly, so it must be done at an earlier stage.
    """

    def __init__(self, ls, symbol):
        """Prepare an odoo builder to parse the symbol"""
        self.ls = ls
        self.symStack = [symbol.get_in_parents([SymType.FILE]) or symbol] # we always load at file level
        self.diagnostics = []
        self.filePath = ""

    def load_odoo_content(self):
        self.diagnostics = []
        if self.symStack[0].odooStatus:
            return
        if self.symStack[0].type in [SymType.NAMESPACE]:
            return
        elif self.symStack[0].type == SymType.PACKAGE:
            self.filePath = os.path.join(self.symStack[0].paths[0], "__init__.py" + self.symStack[0].i_ext)
        else:
            self.filePath = self.symStack[0].paths[0]
        self.symStack[0].odooStatus = 1
        self.symStack[0].validationStatus = 0
        if DEBUG_ODOO_BUILDER:
            print("Load odoo: " + self.filePath)
        fileInfo = FileMgr.get_file_info(self.filePath)
        if not fileInfo.ast: #we don't want to validate it
            return
        self._load()
        fileInfo.replace_diagnostics(BuildSteps.ODOO, self.diagnostics)
        Odoo.get().add_to_validations(self.symStack[0])
        self.symStack[0].odooStatus = 2

    def _load(self):
        for symbol in self.symStack[0].get_ordered_symbols():
            if symbol.type == SymType.CLASS:
                if self._is_model(symbol):
                    self._load_class_inherit(symbol)
                    self._load_class_name(symbol)
                    if not symbol.modelData:
                        continue
                    self._load_class_inherits(symbol)
                    self._load_class_attributes(symbol)
                    model = Odoo.get().models.get(symbol.modelData.name, None)
                    if not model:
                        model = Model(symbol.modelData.name, symbol)
                        Odoo.get().models[symbol.modelData.name] = model
                    else:
                        model.add_symbol(symbol)


    def _load_class_inherit(self, symbol):
        """ load the model inherit list from the class definition """
        raise NotImplementedError

    def _load_class_name(self, symbol):
        """ load the model name from the class definition """
        raise NotImplementedError

    def _load_class_inherits(self, symbol):
        """ load the model inherits list from the class definition """
        raise NotImplementedError

    def _load_class_attributes(self, symbol):
        """ load the model attributes list from the class definition """
        raise NotImplementedError

    def _is_model(self, symbol):
        """return True if the symbol inherit from odoo.models.BaseModel. It differs on the
        is_model on symbol as it can be used before the OdooBuilder execution"""
        if not isinstance(symbol, ClassSymbol):
            print("class is not a ClassSymbol, something is broken")
            return
        baseModel = Odoo.get().get_symbol(["odoo", "models"], ["BaseModel"])
        model = Odoo.get().get_symbol(["odoo", "models"], ["Model"])
        transient = Odoo.get().get_symbol(["odoo", "models"], ["TransientModel"])
        # _register is always set to True at each inheritance, so no need to check for parent classes
        if symbol.inherits(baseModel) and symbol not in [baseModel, model, transient]:
            symbol.modelData = ModelData()
            _register = symbol.get_symbol([], ["_register"])
            if _register and _register.eval:
                value = _register.eval.get_symbol().follow_ref()[0]
                if value.type == SymType.PRIMITIVE:
                    symbol.modelData.register = value.value
                    if value.value == False:
                        return False
            return True
        return False