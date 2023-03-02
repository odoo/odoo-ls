import sys
from server.inferencer import *

class ModelData():

    def __init__(self):
        #data related to model symbols
        self.name = ""
        self.inherit = []
        self.inherits = []
        self.log_access = []

class ClassData():
    
    def __init__(self):
        #data related to classes symbols
        self.bases = [] #list of tree names
        self.modelData = None

class Symbol():
    """A symbol is an object representing an element of the code architecture.
    It can be either a python package, a file, a class, a function, or even a variable.
    All these data are static and no inference of code execution is done.
    By querying a symbol, you will be able to find his sources (file, line of code, etc...), find his
    children (function/variables for a class).

    Some values can be type dependant and not available on each symbol. Please check the documentation of each variable
    to get more information
    """

    def __init__(self, name, type, paths):
        self.name = name
        self.type = type #root, ext_package, package, file, compiled, class, function, variable
        self.evaluationType = None # inferred symbol treename of the type of the variable of function return
        self.paths = paths if isinstance(paths, list) else [paths]
        #symbols is a dictionnary of all symbols that is contained by the current symbol
        self.symbols = {}
        #List of symbols not available from outside as they are redefined later in the same symbol 
        #(ex: two classes with same name in same file. Only last will be available for imports, 
        # but the other can be used locally)
        self.localSymbols = [] 
        self.dependents = []
        self.diagnostics = []
        self.parent = None
        self.isModule = False
        self.classData = None
        self.inferencer = Inferencer()
        self.startLine = 0
        self.endLine = 0
        self.archStatus = 0 #0: not loaded, 1: building, 2: loaded
        self.validationStatus = 0 #0: not validated, 1: in validation, 2: validated
    
    def __str__(self):
        return "(" + self.name + " - " + self.type + " - " + str(self.paths) + ")"
    
    def unload(self):
        from .odoo import Odoo
        #1: collect all symbols to revalidate
        symbols = [self]
        while symbols:
            for d in symbols[0].dependents:
                Odoo.get().add_to_rebuild(d.get_tree())
            for s in symbols[0].symbols.values():
                symbols.append(s)
            del symbols[0]
        #2: delete symbol
        self.parent.symbols.pop(self.name)
        self.type = "dirty" #to help debugging

    def get_tree(self):
        tree = []
        curr_symbol = self
        while curr_symbol.type != "root" and curr_symbol.parent:
            tree.insert(0, curr_symbol.name)
            curr_symbol = curr_symbol.parent
        return tree

    def get_symbol(self, symbol_names):
        """starting from the current symbol, give the symbol corresponding the tree branch symbol_names.
        Example: symbol = symbol.get_symbol(['odoo', 'models', 'Model'])
        will return the symbol corresponding to odoo.models.Model.
        From this one, we can do symbol.get_symbol(['hello']) to get the 'hello' symbol of the Model class"""
        if not symbol_names:
            return self
        if symbol_names[0] in self.symbols:
            curr_symbol = self.symbols[symbol_names[0]]
            if curr_symbol:
                return curr_symbol.get_symbol(symbol_names[1:])
        #last chance, if we are in a file, we can return any declared var
        if self.type not in ["root", "namespace"]:
            inference = self.inferName(symbol_names[0], 99999999)
            if inference and inference.symbol:
                return inference.symbol
        return False

    def getModule(self):
        s = self
        while s and not s.isModule:
            s = s.parent
        return s and s.name or None

    def get_class_symbol(self, name, prevent_comodel = False):
        """Only on type=='class'. Try to find a symbol with the right 'name'. If not present in the symbol, will
        search on bases or on comodels for odoo models"""
        if name in self.symbols:
            return self.symbols[name]
        if self.isModel() and not prevent_comodel:
            from .odoo import Odoo
            model = Odoo.get().models[self.classData.modelData.name]
            sym = model.get_symbols(self.getModule())
            for s in sym:
                r = s.get_class_symbol(name, True)
                if r:
                    return r
        for base in self.classData.bases:
            base_sym = Odoo.get().symbols.get_symbol(base)
            s = base_sym.get_class_symbol(name)
            if s:
                return s
        return None
    
    def is_inheriting_from(self, class_tree):
        if not self.classData:
            return False
        from .odoo import Odoo
        for s in self.classData.bases:
            base_sym = Odoo.get().symbols.get_symbol(s)
            if base_sym.get_tree() == class_tree or base_sym.is_inheriting_from(class_tree):
                return True
        return False

    def add_symbol(self, symbol_names, symbol):
        """take a list of symbols name representing a relative path (ex: odoo.addon.models) and the symbol to add"""
        if symbol_names and symbol_names[0] not in self.symbols:
            raise Exception("Symbol not found: " + str(symbol_names[0]))
        curr_symbol = self.symbols[symbol_names[0]] if symbol_names else self
        for s in symbol_names[1:]:
            if s in curr_symbol.symbols:
                curr_symbol = curr_symbol.symbols[s]
            else:
                raise Exception("Package not found: " + str(symbol_names))
        symbol.parent = curr_symbol
        if symbol.name in curr_symbol.symbols:
            #TODO we don't want to handle this case for now. It can occur for directory of addons
            # because it has already been added, but it can occur too if two files or directories
            # have the same name, or even two same classes in a file. We should handle this case in the future
            print("Symbol already exists: " + str(curr_symbol.get_tree()) + " - " + str(symbol.name)) 
        else:
            curr_symbol.symbols[symbol.name] = symbol
    
    def get_in_parents(self, type, stop_same_file = True):
        if self.type == type:
            return self
        if stop_same_file and self.type in ["file", "package", "ext_package"]: #a __init__.py file is encoded as a Symbol package
            return None
        if self.parent:
            return self.parent.get_in_parents(type, stop_same_file)

    def get_scope_symbol(self, line):
        """return the symbol (class or function) the closest to the given line """
        #TODO search in localSymbols too
        symbol = self
        for s in self.symbols.values():
            if s.startLine <= line and s.endLine >= line:
                symbol = s.get_scope_symbol(line)
                break
        return symbol
    
    def get_class_scope_symbol(self, line):
        """return the class symbol closest to the given line. If the line is not in a class, return None. """
        #TODO search in localSymbols too
        symbol = self
        assert self.type == "file", "can only be called on file symbols"
        if self.type == 'class':
            return self
        for s in self.symbols.values():
            if s.startLine <= line and s.endLine >= line:
                symbol = s.get_class_scope_symbol(line)
                break
        if symbol.type != 'class':
            symbol = None
        return symbol
    
    def inferName(self, name, line):
        #TODO search in localSymbols too?
        local = self.inferencer.inferName(name, line)
        if self.type in ["file", "package", "ext_package"]:
            return local
        if not local:
            return self.parent.inferName(name, line)
        return local
    
    def isClass(self):
        return bool(self.classData)
    
    def isModel(self):
        return self.isClass() and bool(self.classData.modelData)

class RootSymbol(Symbol):

    def add_symbol(self, symbol_names, symbol):
        """take a list of symbols name representing a relative path (ex: odoo.addon.models) and the symbol to add"""
        super().add_symbol(symbol_names, symbol)
        if not symbol_names:
            for path in symbol.paths:
                for sysPath in sys.path:
                    if sysPath == "":
                        continue
                    if path.startswith(sysPath):
                        symbol.type = "ext_package"
                        return