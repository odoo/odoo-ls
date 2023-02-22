from .symbol import Symbol
from .odoo import *

class Model():

    def __init__(self, name, symbol):
        self.name = name
        self.impl_sym = [symbol]
    
    def get_main_symbol(self):
        return self.impl_sym[0]
    
    def get_symbols(self, module_scope_name):
        """Return a list of symbols that extends this model but are in your dependencies."""
        symbols = []
        module_scope = Odoo.get().modules.get(module_scope_name, False)
        if not module_scope:
            return symbols
        for symbol in self.impl_sym:
            module = symbol.getModule()
            if module_scope:
                if module and module_scope.is_in_deps(module):
                    symbols.append(symbol)
            else:
                symbols.append(symbol)
        return symbols
    
    def get_inherit(self, module_scope_name):
        """Return a list of model names that are inherited by this model.
        If module_scope is not None, only return inheritance coming from files that are in dependencies"""
        inherit = set()
        module_scope = Odoo.get().modules.get(module_scope_name, False)
        if not module_scope:
            return [] #TODO maybe raise an error
        for symbol in self.impl_sym:
            module = symbol.getModule()
            if module_scope:
                if module and module_scope.is_in_deps(module):
                    inherit.update(symbol.inherit)
            else:
                inherit.update(symbol.inherit)
        return list(inherit)

