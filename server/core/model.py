from .odoo import Odoo
from ..references import RegisteredRefSet

class Model():

    def __init__(self, name, symbol):
        self.name = name
        self.impl_sym = RegisteredRefSet()
        self.add_symbol(symbol)

    def add_symbol(self, symbol):
        self.impl_sym.add(symbol)

    def get_main_symbols(self, from_module = None, module_acc:set = None):
        """Return all the symbols that declare the module in the dependencies of the from_module, or all main symbols
        if from_module is None.
        A module_acc can be given to speedup the process. It should be a set of module names that are in the dependencies
        """
        res = []
        for sym in self.impl_sym:
            if sym.modelData.name not in sym.modelData.inherit:
                if not from_module or \
                (module_acc and sym.get_module_sym().dir_name in module_acc) or \
                from_module.is_in_deps(sym.get_module_sym().dir_name, module_acc):
                    res.append(sym)
                    if module_acc is not None:
                        module_acc.add(sym.get_module_sym().dir_name)
        return res

    def is_abstract(self, from_module = None, module_acc:set=None):
        main_symbol = self.get_main_symbols(from_module, module_acc)
        if main_symbol and len(main_symbol) == 1:
            for base in main_symbol[0].bases:
                if base.name == 'BaseModel': #TODO not perfect, what about ancestors? what about an "abstract = False" attribute?
                    return True
                else:
                    return False
        return False

    def get_documentation(self, from_module = None, module_acc:set=None):
        main_symbol = self.get_main_symbols(from_module, module_acc)
        if main_symbol and len(main_symbol) == 1:
            description = main_symbol[0].get_member_symbol("_description", from_module=from_module, prevent_comodel=False)
            description_text = main_symbol[0].name
            if description:
                description, _ = description.follow_ref()
                if description:
                    description_text = description.value or main_symbol[0].name
            return description_text + ": " + ((main_symbol[0].doc and main_symbol[0].doc.value) or "")
        return ""

    def get_symbols(self, from_module):
        """Return a list of symbols that extends this model but are in your dependencies."""
        symbols = []
        for symbol in self.impl_sym:
            module = symbol.get_module_sym()
            if from_module:
                if module and from_module.is_in_deps(module.dir_name):
                    symbols.append(symbol)
            else:
                symbols.append(symbol)
        return symbols

    def get_inherit(self, from_module):
        """Return a list of model names that are inherited by this model.
        If module_scope is not None, only return inheritance coming from files that are in dependencies"""
        inherit = set()
        for symbol in self.impl_sym:
            module = symbol.get_module_sym()
            if from_module:
                if module and from_module.is_in_deps(module.dir_name):
                    inherit.update(symbol.inherit)
            else:
                inherit.update(symbol.inherit)
        return list(inherit)

    def get_attributes(self, from_module, _evaluated = set()):
        """Return all attributes that are in the model from the "from_module" perspective"""
        impl = self.get_symbols(from_module)
        _evaluated.add(self.name)
        #TODO respect dependencies and don't take overrding functions
        #TODO return only fields variables
        res = {}
        for sym in impl:
            for sub_sym in sym.all_symbols(include_inherits=True):
                res[sub_sym.name] = sub_sym
            # model inheritances
            if sym.modelData:
                if sym.modelData.inherit:
                    for inherit in sym.modelData.inherit:
                        if inherit not in _evaluated:
                            inherit_model = Odoo.get().models.get(inherit, None)
                            if inherit_model:
                                for sub_sym in inherit_model.get_attributes(from_module, _evaluated):
                                    res[sub_sym.name] = sub_sym
        return res.values()

    def get_base_distance(self, name, from_module):
        pass

###################################
#  Symbol compatibility methods   #
###################################
# These methods are there to allow usage of Model objects as Symbol objects

    def is_model(self): #TODO beurk, it doesn't mean that on Symbol
        return True

    def get_member_symbol(self, name, from_module, all=False):
        """ Return the first definition of the name in the model from the "from_module" perspective"""
        impl = self.get_symbols(from_module)
        #TODO actually we are searching for the first one, and we could return an override. It would be better if we
        #could search in sorted modules (by dep)
        for sym in impl:
            res = sym.get_member_symbol(name, all=all)
            if res:
                return res
        return None