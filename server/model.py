import weakref
from .symbol import Symbol
from .odoo import *

class Model():

    def __init__(self, name, symbol):
        self.name = name
        self.impl_sym = weakref.WeakSet()
        self.add_symbol(symbol)
    
    def add_symbol(self, symbol):
        self.impl_sym.add(symbol)
    
    def get_main_symbols(self, from_module):
        """Return all the symbols that declare the module in the dependencies of the from_module"""
        res = []
        for sym in self.impl_sym:
            if sym.modelData.name not in sym.modelData.inherit:
                if from_module.is_in_deps(sym.getModule().name):
                    res.append(sym)
        return res
    
    def get_symbols(self, from_module):
        """Return a list of symbols that extends this model but are in your dependencies."""
        symbols = []
        for symbol in self.impl_sym:
            module = symbol.getModule()
            if from_module:
                if module and from_module.is_in_deps(module.name):
                    symbols.append(symbol)
            else:
                symbols.append(symbol)
        return symbols
    
    def get_inherit(self, from_module):
        """Return a list of model names that are inherited by this model.
        If module_scope is not None, only return inheritance coming from files that are in dependencies"""
        inherit = set()
        for symbol in self.impl_sym:
            module = symbol.getModule()
            if from_module:
                if module and from_module.is_in_deps(module.name):
                    inherit.update(symbol.inherit)
            else:
                inherit.update(symbol.inherit)
        return list(inherit)

