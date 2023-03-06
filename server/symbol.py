import sys
import weakref
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
        self.type = type #root, package, file, compiled, class, function, variable
        self.evaluationType = None # actually either weakrefof a symbol or the symbol of a primitive (value stored in evaluationType of this one)
        self.paths = paths if isinstance(paths, list) else [paths]
        #symbols and moduleSymbols is a dictionnary of all symbols that is contained by the current symbol
        #symbols contains classes, functions, variables (all file content)
        self.symbols = {}
        #moduleSymbols contains namespace, packages, files
        self.moduleSymbols = {}
        #List of symbols not available from outside as they are redefined later in the same symbol 
        #(ex: two classes with same name in same file. Only last will be available for imports, 
        # but the other can be used locally)
        self.localSymbols = [] 
        self.dependents = weakref.WeakSet()
        self.diagnostics = []
        self.parent = None
        self.isModule = False
        self.classData = None
        self.external = False
        self.inferencer = Inferencer()
        self.startLine = 0
        self.endLine = 0
        self.archStatus = 0 #0: not loaded, 1: building, 2: loaded
        self.validationStatus = 0 #0: not validated, 1: in validation, 2: validated
    
    def __str__(self):
        return "(" + self.name + " - " + self.type + " - " + str(self.paths) + ")"
    
    def all_symbols(self):
        for s in self.symbols.values():
            yield s
        for s in self.moduleSymbols.values():
            yield s
    
    def is_file_content(self):
        return self.type not in ["namespace", "package", "file", "compiled"]
    
    def unload(self):
        from .odoo import Odoo
        #1: collect all symbols to revalidate
        symbols = [self]
        while symbols:
            for d in symbols[0].dependents:
                Odoo.get().add_to_rebuild(d)
            for s in symbols[0].all_symbols():
                symbols.append(s)
            del symbols[0]
        #2: delete symbol
        self.parent.symbols.pop(self.name)
        self.type = "dirty" #to help debugging

    def get_tree(self):
        tree = ([], [])
        curr_symbol = self
        while curr_symbol.type != "root" and curr_symbol.parent:
            if curr_symbol.is_file_content():
                if tree[0]:
                    print("impossible") #TODO remove this test
                tree[1].insert(0,  curr_symbol.name)
            else:
                tree[0].insert(0,  curr_symbol.name)
            curr_symbol = curr_symbol.parent
        return tree

    def get_symbol(self, symbol_tree_files, symbol_tree_content = [], excl=None):
        """starting from the current symbol, give the symbol corresponding to the right tree branch.
        Example: symbol = symbol.get_symbol(['odoo', 'models'], ['Model'])
        symbol_tree_files are parts that are mandatory "on disk": files, packages, namespaces.
        symbol_tree_content is the parts that are 1) from the content of a file, and if not found
        2) a symbol_tree_files.
        If you don't know the type of data you are searching for, just use the second parameter.
        This implementation allows to fix ambiguity in the case of a package P holds a symbol A
        in its __init__.py and a file A.py in the directory. An import from elswhere that would 
        type 'from P.A import c' would have to call get_symbol(["P", "A"], ["c"]) because P and A
        can't be file content (because theyr're in the from clause)"""
        #This function of voluntarily non recursive
        if isinstance(symbol_tree_files, str) or isinstance(symbol_tree_content, str):
            raise Exception("get_symbol can only be used with list")
        current_symbol = self
        while symbol_tree_files or symbol_tree_content:
            if symbol_tree_files:
                next_sym = current_symbol.moduleSymbols.get(symbol_tree_files[0], None)
                if next_sym:
                    current_symbol = next_sym
                    symbol_tree_files = symbol_tree_files[1:]
                    continue
                return None
            next_sym = current_symbol.symbols.get(symbol_tree_content[0], None)
            if next_sym and current_symbol != excl:
                current_symbol = next_sym
                symbol_tree_content = symbol_tree_content[1:]
            elif current_symbol.type == "compiled":
                # always accept symbols in compiled files
                return current_symbol
            else:
                next_sym = current_symbol.moduleSymbols.get(symbol_tree_content[0], None)
                if next_sym:
                    current_symbol = next_sym
                    symbol_tree_content = symbol_tree_content[1:]
                else:
                    return None
        return current_symbol

        #last chance, if we are in a file, we can return any declared var
        if self.type not in ["root", "namespace"]:
            inference = self.inferName(symbol_names[0], 99999999)
            if inference and inference.symbol:
                return inference.symbol

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

    def add_symbol(self, symbol):
        """take the symbol to add"""
        sym_dict = self.moduleSymbols
        if symbol.is_file_content():
            sym_dict = self.symbols
        symbol.parent = self
        if symbol.name in sym_dict:
            #TODO we don't want to handle this case for now. It can occur for directory of addons
            # because it has already been added, but it can occur too if two files or directories
            # have the same name, or even two same classes in a file. We should handle this case in the future
            pass # print("Symbol already exists: " + str(self.get_tree()) + " - " + str(symbol.name)) 
        else:
            sym_dict[symbol.name] = symbol
        
    def add_module_symbol(self, symbol_names, symbol):
        pass
    
    def get_in_parents(self, types, stop_same_file = True):
        if self.type in types:
            return self
        if stop_same_file and self.type in ["file", "package"]: #a __init__.py file is encoded as a Symbol package
            return None
        if self.parent:
            return self.parent.get_in_parents(types, stop_same_file)

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
        if self.type in ["file", "package"]:
            return local
        if not local:
            return self.parent.inferName(name, line)
        return local
    
    def isClass(self):
        return bool(self.classData)
    
    def isModel(self):
        return self.isClass() and bool(self.classData.modelData)
    
    def is_external(self):
        if self.external:
            return True
        if self.parent:
            return self.parent.is_external()
        return False

class RootSymbol(Symbol):

    def add_symbol(self, symbol):
        """take a list of symbols name representing a relative path (ex: odoo.addon.models) and the symbol to add"""
        super().add_symbol(symbol)
        if symbol.type in ["package", "file"]:
            for path in symbol.paths:
                for sysPath in sys.path:
                    if sysPath == "":
                        continue
                    if path.startswith(sysPath):
                        symbol.external = True
                        return