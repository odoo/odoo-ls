from server.inferencer import *

class ModelData():

    def __init__(self):
        #data related to model symbols
        self.name
        self.inherit = []
        self.inherits = []
        self.log_access = []

class ClassData():
    
    def __init__(self):
        #data related to classes symbols
        self.bases = []
        self.modelData = None
    
    def is_inheriting_from(self, class_name):
        for s in self.bases:
            if s.get_tree() == class_name or s.is_inheriting_from(class_name):
                return True
        return False

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
        self.type = type #root, package, file, class, function, variable
        self.evaluationType = None # inferred symbol of the type of the variable of function return
        self.paths = paths if isinstance(paths, list) else [paths]
        #symbols is a dictionnary of all symbols that is contained by the current symbol
        self.symbols = {}
        self.parent = None
        self.isModule = False
        self.classData = None
        self.inferencer = Inferencer()
        self.startLine = 0
        self.endLine = 0
    
    def __str__(self):
        return "(" + self.name + " - " + self.type + " - " + str(self.paths) + ")"

    def get_tree(self):
        ancestors = []
        curr_symbol = self
        while curr_symbol.parent and curr_symbol.parent.type != "root":
            ancestors.insert(0, curr_symbol.parent.name)
            curr_symbol = curr_symbol.parent
        return ancestors + [self.name]

    def get_symbol(self, symbol_names):
        if not symbol_names:
            return self
        if symbol_names[0] in self.symbols:
            curr_symbol = self.symbols[symbol_names[0]]
            if curr_symbol:
                return curr_symbol.get_symbol(symbol_names[1:])
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
        if self.modelName and not prevent_comodel:
            from .odoo import Odoo
            model = Odoo.get().models[self.modelName]
            sym = model.get_symbols(self.getModule())
            for s in sym:
                r = s.get_class_symbol(name, True)
                if r:
                    return r
        for base in self.classData.bases:
            s = base.get_class_symbol(name)
            if s:
                return s
        return None

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
            print("Symbol already exists") #TODO is it correct? shouldn't we merge paths?
        else:
            curr_symbol.symbols[symbol.name] = symbol
    
    def get_in_parents(self, type, stop_same_file = True):
        if self.type == type:
            return self
        if stop_same_file and self.type in ["file", "package"]: #a __init__.py file is encoded as a Symbol package
            return None
        return self.parent.get_in_parents(type, stop_same_file)

    def get_scope_symbol(self, line):
        """return the symbol (class or function) the closest to the given line """
        symbol = self
        for s in self.symbols.values():
            if s.startLine <= line and s.endLine >= line:
                symbol = s.get_scope_symbol(line)
                break
        return symbol
    
    def get_class_scope_symbol(self, line):
        """return the class symbol closest to the given line. If the line is not in a class, return None. """
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