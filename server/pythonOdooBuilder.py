import ast
import os
from .constants import *
from .odoo import *
from .server import FileMgr


class ClassContentCache():

    def __init__(self):
        self.modelName = None
        self.modelInherit = []
        self.modelInherits = []
        self.log_access = True


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
            self.filePath = os.path.join(self.symStack[0].paths[0], "__init__.py")
        else:
            self.filePath = self.symStack[0].paths[0]
        self.symStack[0].odooStatus = 1
        if (not Odoo.get().isLoading):
            print("Load odoo: " + self.filePath)
        self.symStack[0].not_found_paths = []
        Odoo.get().not_found_symbols.discard(self.symStack[0])
        fileInfo = FileMgr.getFileInfo(self.filePath)
        if not fileInfo["ast"]: #doesn"t compile or we don't want to validate it
            return
        self._load()
        fileInfo["d_odoo"] = self.diagnostics
        Odoo.get().to_validate.add(self.symStack[0])
        self.symStack[0].odooStatus = 2
        #never publish diagnostics? if a odooBuilder is involved, a validation should be too, so we can publish them together
        #FileMgr.publish_diagnostics(self.ls, fileInfo)

    def _load(self):
        for symbol in self.symStack[0].get_ordered_symbols():
            if symbol.type == SymType.CLASS:
                if self.is_model(symbol):
                    pass

    def is_model(self, symbol):
        """return True if the symbol inherit from odoo.models.BaseModel"""
        if not symbol.classData:
            print("class has no classData, something is broken")
            return
        for base in symbol.classData.bases:
            pass