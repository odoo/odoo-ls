import gc
import os
import sys
from ..references import RegisterableObject, RegisteredRef, RegisteredRefSet, RegisteredRefList
from ..odoo_language_server import OdooLanguageServer
from ..constants import *

import base64

class ModelData():

    def __init__(self):
        #data related to model symbols
        self.name = ""
        self.description = ""
        self.inherit = []
        self.inherits = []
        self.register = False
        self.auto = False
        self.log_access = False
        self.table = False
        self.sequence = None
        self.sql_constraints = []
        self.abstract = False
        self.transient = False
        self.rec_name = None
        self.order = 'id'
        self.check_company_auto = False
        self.parent_name = 'parent_id'
        self.parent_store = False
        self.data_name = 'date'
        self.fold_name = 'fold'


class Symbol(RegisterableObject):
    """A symbol is an object representing an element of the code architecture.
    It can be either a python package, a file, a class, a function, or even a variable.
    All these data are static and no inference of code execution is done.
    By querying a symbol, you will be able to find his sources (file, line of code, etc...), find his
    children (function/variables for a class).

    Some values can be type dependant and not available on each symbol. Please check the documentation of each variable
    to get more information
    """

    __slots__ = ("name", "type", "eval", "ast_node", "value", "symbols", "moduleSymbols",
        "localSymbols",  "dependencies", "dependents", "parent",
        "modelData", "external", "start_pos", "end_pos", "archStatus", "odooStatus", "validationStatus",
        "not_found_paths", "i_ext", "doc", "deprecated_reason")

    def __init__(self, name, type):
        super().__init__()
        self.name = name
        self.type: SymType = type
        self.eval = None
        self.i_ext = "" # indicates if i should be added at the end of the path (for __init__.pyi for example)
        self.ast_node = None
        self.value = None #ref to ast node that can be used to evalute the symbol
        #symbols and moduleSymbols is a dictionnary of all symbols that is contained by the current symbol
        #symbols contains classes, functions, variables (all file content)
        self.symbols = {}
        #moduleSymbols contains namespace, packages, files
        self.moduleSymbols = {}
        #List of symbols not available from outside as they are redefined later in the same symbol
        #(ex: two classes with same name in same file. Only last will be available for imports,
        # but the other can be used locally)
        self.localSymbols = RegisteredRefList()
        if self.type in (SymType.PACKAGE, SymType.FILE):
            self.dependencies = { #symbol that are needed to build this symbol
                BuildSteps.ARCH: { #symbols needed to build arch of this symbol
                    BuildSteps.ARCH: RegisteredRefSet(),
                },
                BuildSteps.ARCH_EVAL: {
                    BuildSteps.ARCH: RegisteredRefSet(),
                },
                BuildSteps.ODOO:{
                    BuildSteps.ARCH: RegisteredRefSet(),
                    BuildSteps.ARCH_EVAL: RegisteredRefSet(),
                    BuildSteps.ODOO: RegisteredRefSet()
                },
                BuildSteps.VALIDATION: {
                    BuildSteps.ARCH: RegisteredRefSet(),
                    BuildSteps.ARCH_EVAL: RegisteredRefSet(),
                    BuildSteps.ODOO: RegisteredRefSet(),
                }
            }
            self.dependents = {
                BuildSteps.ARCH: {
                    BuildSteps.ARCH: RegisteredRefSet(),
                    BuildSteps.ARCH_EVAL: RegisteredRefSet(),
                    BuildSteps.ODOO: RegisteredRefSet(),
                    BuildSteps.VALIDATION: RegisteredRefSet() #set of symbol that need to be rebuilt when this symbol is modified at arch level
                },
                BuildSteps.ARCH_EVAL: { #set of symbol that need to be rebuilt when this symbol is re-evaluated
                    #BuildSteps.ARCH_EVAL: RegisteredRefSet(), #should not occur? if yes, check that rebuild order is not sometimes broken
                    BuildSteps.ODOO: RegisteredRefSet(),
                    BuildSteps.VALIDATION: RegisteredRefSet()
                },
                BuildSteps.ODOO: { #set of symbol that need to be rebuilt when this symbol is modified at Odoo level
                    BuildSteps.ODOO: RegisteredRefSet(),
                    BuildSteps.VALIDATION: RegisteredRefSet()
                }
            }
        self.parent = None
        self.modelData = None
        self.external = False #use is_external()
        self.in_workspace = False
        self.start_pos = (0, 0) # (line, column)
        self.end_pos = (0, 0) # (line, column)
        self.archStatus = 0 #0: not loaded, 1: building, 2: loaded
        self.evalStatus = 0
        self.odooStatus = 0 #0: not loaded, 1: building, 2: loaded
        self.validationStatus = 0 #0: not validated, 1: in validation, 2: validated
        self.not_found_paths = []
        self.deprecated_reason: str = None #if not None, the symbol is deprecated and the reason is given
        self.doc = None

    def __str__(self):
        return "(" + self.name + " - " + str(self.type) + " - " + str(self.get_paths()) + ")"

    def __del__(self):
        if DEBUG_MEMORY:
            print("symbol deleted " + self.name + " at " + os.sep.join(self.get_paths()))

    def all_symbols(self, line=-1, include_inherits=False):
        if line != -1:
            for s in self.localSymbols:
                if s.start_pos[0] < line:
                    yield s
        if include_inherits:
            for sub_s in self._all_symbols_from_class(line=line):
                yield sub_s
        for s in self.symbols.values():
            if line == -1 or s.start_pos[0] < line:
                yield s
        for s in self.moduleSymbols.values():
            yield s

    def _all_symbols_from_class(self, line=-1):
        return []

    def follow_ref(self, context=None):
        from .python_arch_eval import PythonArchEval
        from .odoo import Odoo
        #follow the reference to the real symbol and returns it (not a RegisteredRef)
        sym = self
        can_eval_external = not self.is_external()
        instance = self.type in [SymType.VARIABLE]
        file = sym.get_in_parents([SymType.FILE, SymType.PACKAGE])
        if sym.eval == None and (not sym.is_external() or can_eval_external) and file and file.evalStatus == 0 and file in Odoo.get().rebuild_arch_eval: #TODO shouldn't we launch arch builder in case of not in rebuild_arch_eval?
            ev = PythonArchEval(OdooLanguageServer.get(), sym.get_in_parents([SymType.FILE, SymType.PACKAGE]))
            ev.eval_arch()
        while sym and sym.type == SymType.VARIABLE and sym.eval and sym.eval.get_symbol_rr(context):
            instance = sym.eval.instance
            if sym.eval.context and context:
                context.update(sym.eval.context)
            sym = sym.eval.get_symbol(context)
            file = sym.get_in_parents([SymType.FILE, SymType.PACKAGE])
            if sym.eval == None and (not sym.is_external() or can_eval_external)  and file and file.evalStatus == 0 and file in Odoo.get().rebuild_arch_eval:
                ev = PythonArchEval(OdooLanguageServer.get(), sym.get_in_parents([SymType.FILE, SymType.PACKAGE]))
                ev.eval_arch()
        return sym, instance

    def add_dependency(self, other_symbol, on_step, dep_level):
        #on this symbol, add a dependency on the steps of other_symbol, for the dep_level.
        # the build of the step "on_step" of self require dep_level of other_symbol to be done
        parent_sym = self.get_in_parents([SymType.FILE, SymType.PACKAGE])
        parent_other_sym = other_symbol.get_in_parents([SymType.FILE, SymType.PACKAGE])
        if parent_sym == parent_other_sym or not parent_other_sym:
            return
        parent_sym.dependencies[on_step][dep_level].add(parent_other_sym)
        parent_other_sym.dependents[dep_level][on_step].add(parent_sym)

    def is_file_content(self):
        return self.type not in [SymType.NAMESPACE, SymType.PACKAGE, SymType.FILE, SymType.COMPILED]

    def get_range(self):
        return (self.start_pos, self.end_pos)

    @staticmethod
    def unload(symbol): #can't delete because of self? :o
        """Unload the symbol and his children. Mark all dependents symbol as 'to revalidate'."""
        from .odoo import Odoo
        from .module import ModuleSymbol
        if symbol.type == SymType.DIRTY:
            print("trying to unload a dirty symbol, skipping")
            return
        to_unload = [symbol]
        while to_unload:
            sym = to_unload[0]
            #1: collect all symbols to revalidate
            found_one = False
            for s in sym.all_symbols(line=9999999999):
                found_one = True
                to_unload.insert(0, s)
            if found_one:
                continue
            else:
                to_unload.remove(sym)

            if DEBUG_MEMORY:
                print("unloading " + sym.name + " at " + os.sep.join(sym.get_paths()))
            #no more children at this point, start unloading the symbol
            sym.parent.remove_symbol(sym)
            #add other symbols related to same ast node (for "import *" nodes)
            # ast_node = sym.ast_node
            # if ast_node and hasattr(ast_node, "linked_symbols"):
            #     for s in ast_node.linked_symbols:
            #         if s != sym:
            #             to_unload.append(s)
            #     ast_node.linked_symbols.clear()
            if isinstance(sym, ModuleSymbol):
                Odoo.get().modules.pop(sym.dir_name, None)
            if sym.type in [SymType.FILE, SymType.PACKAGE]:
                sym.invalidate(BuildSteps.ARCH)
            #if DEBUG_MEMORY:
            #    print("is now dirty : " + sym.name + " at " + os.sep.join(sym.get_paths()))
            sym.localSymbols.clear()
            sym.moduleSymbols.clear()
            sym.symbols.clear()
            sym.parent = None
            sym.type = SymType.DIRTY
            sym.mark_as_deleted()
            del sym

    def invalidate(self, step):
        #signal that a change occur to this symbol. "step" indicates which level of change occured.
        #it can be arch, arch_eval, odoo or validation
        from .odoo import Odoo
        symbols = [self]
        while symbols:
            sym_to_invalidate = symbols.pop(0)
            if sym_to_invalidate.type in [SymType.FILE, SymType.PACKAGE]:
                if step == BuildSteps.ARCH:
                    for to_rebuild_level, syms in sym_to_invalidate.dependents[BuildSteps.ARCH].items():
                        for sym in syms:
                            if sym != self and not sym.is_symbol_in_parents(self):
                                if to_rebuild_level == BuildSteps.ARCH:
                                    Odoo.get().add_to_arch_rebuild(sym)
                                elif to_rebuild_level == BuildSteps.ARCH_EVAL:
                                    Odoo.get().add_to_arch_eval(sym)
                                elif to_rebuild_level == BuildSteps.ODOO:
                                    Odoo.get().add_to_init_odoo(sym)
                                elif to_rebuild_level == BuildSteps.VALIDATION:
                                    Odoo.get().add_to_validations(sym)
                if step in [BuildSteps.ARCH, BuildSteps.ARCH_EVAL]:
                    for to_rebuild_level, syms in sym_to_invalidate.dependents[BuildSteps.ARCH_EVAL].items():
                        for sym in syms:
                            if sym != self and not sym.is_symbol_in_parents(self):
                                if to_rebuild_level == BuildSteps.ARCH_EVAL:
                                    Odoo.get().add_to_arch_eval(sym)
                                elif to_rebuild_level == BuildSteps.ODOO:
                                    Odoo.get().add_to_init_odoo(sym)
                                elif to_rebuild_level == BuildSteps.VALIDATION:
                                    Odoo.get().add_to_validations(sym)
                if step in [BuildSteps.ARCH, BuildSteps.ARCH_EVAL, BuildSteps.ODOO]:
                    for to_rebuild_level, syms in sym_to_invalidate.dependents[BuildSteps.ODOO].items():
                        for sym in syms:
                            if sym != self and not sym.is_symbol_in_parents(self):
                                if to_rebuild_level == BuildSteps.ODOO:
                                    Odoo.get().add_to_init_odoo(sym)
                                elif to_rebuild_level == BuildSteps.VALIDATION:
                                    Odoo.get().add_to_validations(sym)
            for s in sym_to_invalidate.all_symbols(line=99999999999):
                symbols.append(s)

    def remove_symbol(self, symbol):
        if symbol.is_file_content():
            in_symbols = self.symbols.get(symbol.name, None)
            if in_symbols:
                if symbol == in_symbols:
                    #if DEBUG_MEMORY:
                    #    print("symbols - remove " + symbol.name + " from " + os.sep.join(self.get_paths()))
                    del self.symbols[symbol.name]
                    if symbol.parent and self.parent == self:
                        symbol.parent = None
                    last = None
                    for localSym in self.localSymbols:
                        if localSym.name == symbol.name:
                            if not last or last.start_pos[0] < localSym.start_pos[0]:
                                last = localSym
                    if last:
                        #if DEBUG_MEMORY:
                        #    print("move sym - " + symbol.name + " from " + os.sep.join(self.get_paths()))
                        self.symbols[symbol.name] = last
                        self.localSymbols.remove(last)
                else:
                    #ouch, the wanted symbol is not in Symbols. let's try to find it in localSymbols
                    try:
                        self.localSymbols.remove(symbol)
                        if symbol.parent and self.parent == self:
                            symbol.parent = None
                        #if DEBUG_MEMORY:
                        #    print("localSymbols - remove " + symbol.name + " from " + os.sep.join(self.get_paths()))
                    except ValueError:
                        if DEBUG_MEMORY:
                            print("Symbol to delete not found")
        else:
            if symbol.name in self.moduleSymbols:
                #if DEBUG_MEMORY:
                #    print("moduleSymbols - remove " + symbol.name + " from " + os.sep.join(self.get_paths()))
                if symbol.parent and self.parent == self:
                    symbol.parent = None
                del self.moduleSymbols[symbol.name]


    def get_tree(self):
        tree = ([], [])
        curr_symbol = self
        while curr_symbol.type != SymType.ROOT and curr_symbol.parent:
            if curr_symbol.is_file_content():
                if tree[0]:
                    print("impossible") #TODO remove this test
                tree[1].insert(0,  curr_symbol.name)
            else:
                tree[0].insert(0,  curr_symbol.name)
            curr_symbol = curr_symbol.parent
        return tree

    def get_symbol(self, symbol_tree_files, symbol_tree_content = []):
        """starting from the current symbol, give the symbol corresponding to the right tree branch.
        Example: symbol = symbol.get_symbol(['odoo', 'models'], ['Model'])
        symbol_tree_files are parts that are mandatory "on disk": files, packages, namespaces.
        symbol_tree_content is the parts that are 1) from the content of a file, and if not found
        2) a symbol_tree_files.
        If you don't know the type of data you are searching for, just use the second parameter.
        This implementation allows to fix ambiguity in the case of a package P holds a symbol A
        in its __init__.py and a file A.py in the directory. An import from elswhere that would
        type 'from P.A import c' would have to call get_symbol(["P", "A"], ["c"]) because P and A
        can't be file content (because theyr're in the from clause)
        in-deep note: it does not respect the precedence of packages over modules. If you have
        a/foo.py and a/foo/test.py, calling get_symbol([], ["a", "foo", "test"]) will return the content of
        the file, but a true import return the test.py file. BUT, as foo.py should be impossible to import,
        it should be not available in the tree, and so the directory is taken
        """
        #This function is voluntarily non recursive
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
            if next_sym:
                current_symbol = next_sym
                symbol_tree_content = symbol_tree_content[1:]
            elif current_symbol.type == SymType.COMPILED:
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

    def get_module_sym(self):
        from .module import ModuleSymbol
        s = self
        while s and not isinstance(s, ModuleSymbol):
            s = s.parent
        return s

    def get_eval(self):
        return self.eval

    def get_model(self):
        if self.is_model():
            from .odoo import Odoo
            return Odoo.get().models[self.modelData.name]
        return None

    def get_member_symbol(self, name, from_module=False, prevent_local=False, prevent_comodel = False, all=False):
        """similar to get_symbol: will return the symbol that is under this one with the specified name.
        However, if the symbol is a class or a model, it will search in the base class or in comodel classes
        if not all, it will return the first found. If all, the all found symbols are returned, but the first one
        is the one that is overriding others.
        :param: from_module: optional, can change the from_module of the given class"""
        from .odoo import Odoo
        res = []
        if name in self.moduleSymbols:
            # if a moduleSymbols AND a symbol are present, the moduleSymbol is taken first
            if not all:
                return self.moduleSymbols[name]
            else:
                res.append(self.moduleSymbols[name])
        if not prevent_local:
            if name in self.symbols:
                if not all:
                    return self.symbols[name]
                else:
                    res.append(self.symbols[name])
        if self.is_model() and not prevent_comodel:
            model = Odoo.get().models[self.modelData.name]
            sym = model.get_symbols(from_module or self.get_module_sym())
            for s in sym:
                if s == self:
                    continue
                r = s.get_member_symbol(name, prevent_local=False, prevent_comodel=True, all=all)
                if r:
                    if not all:
                        return r
                    else:
                        for r_iter in r:
                            if r_iter not in res:
                                res.append(r_iter)
        return res

    def is_inheriting_from(self, class_tree):
        return False

    def add_symbol(self, symbol):
        """take the symbol to add"""
        sym_dict = self.moduleSymbols
        if symbol.is_file_content():
            sym_dict = self.symbols
        symbol.parent = self
        if symbol.name not in sym_dict:
            sym_dict[symbol.name] = symbol
        elif symbol.is_file_content():
            if symbol.start_pos[0] < self.symbols[symbol.name].start_pos[0]:
                self.localSymbols.append(symbol)
            else:
                self.symbols[symbol.name].invalidate(BuildSteps.ARCH)
                self.localSymbols.append(self.symbols[symbol.name])
                self.symbols[symbol.name] = symbol

    def add_module_symbol(self, symbol_names, symbol):
        pass

    def get_in_parents(self, types, stop_same_file = True):
        if self.type in types:
            return self
        if stop_same_file and self.type in [SymType.FILE, SymType.PACKAGE]: #a __init__.py file is encoded as a Symbol package
            return None
        if self.parent:
            return self.parent.get_in_parents(types, stop_same_file)

    def is_symbol_in_parents(self, symbol):
        while self.parent != symbol and self.parent:
            self = self.parent
        return self.parent == symbol

    def get_scope_symbol(self, line):
        """return the symbol (class or function) the closest to the given line """
        #TODO search in localSymbols too
        symbol = self
        for s in self.symbols.values():
            if s.start_pos[0] < line and s.end_pos[0] >= line and s.type in [SymType.CLASS, SymType.FUNCTION]:
                symbol = s.get_scope_symbol(line)
                break
            elif s.start_pos[0] > line:
                break
        return symbol

    def get_class_scope_symbol(self, line):
        """return the class symbol closest to the given line. If the line is not in a class, return None. """
        #TODO search in localSymbols too
        symbol = self
        assert self.type == "file", "can only be called on file symbols"
        if self.type == SymType.CLASS:
            return self
        for s in self.symbols.values():
            if s.start_pos[0] <= line and s.end_pos[0] >= line:
                symbol = s.get_class_scope_symbol(line)
                break
        if symbol.type != SymType.CLASS:
            symbol = None
        return symbol

    def infer_name(self, name, line):
        selected = False
        if name == "__doc__":
            return self.doc
        for symbol in self.all_symbols(line=line):
            if symbol.name == name and (not selected or symbol.start_pos[0] > selected.start_pos[0]):
                selected = symbol
        if not selected and self.type not in [SymType.FILE, SymType.PACKAGE]:
            return self.parent.infer_name(name, line)
        if not selected and (self.name != "builtins" or self.type != SymType.FILE):
            #search in builtins
            from .odoo import Odoo
            return Odoo.get().builtins.get_symbol(["builtins"]).infer_name(name, -1)
        return selected

    def is_model(self):
        return self.type == SymType.CLASS and bool(self.modelData)

    def is_external(self):
        if self.external:
            return True
        if self.parent:
            return self.parent.is_external()
        return False

    def get_ordered_symbols(self):
        """return all symbols from local, moduleSymbols and symbols ordered by line declaration"""
        symbols = []
        for s in self.localSymbols:
            symbols.append(s)
        for s in self.moduleSymbols.values():
            symbols.append(s)
        for s in self.symbols.values():
            symbols.append(s)
        return sorted(symbols, key=lambda x: x.start_pos[0])

    def get_paths(self):
        if self.parent:
            return self.parent.get_paths()
        return []

class RootSymbol(Symbol):

    def add_symbol(self, symbol):
        """take a list of symbols name representing a relative path (ex: odoo.addon.models) and the symbol to add"""
        from .odoo import Odoo
        super().add_symbol(symbol)
        if symbol.type in [SymType.FILE, SymType.PACKAGE]:
            for path in symbol.paths:
                for sysPath in sys.path:
                    if sysPath == "":
                        continue
                    if path.startswith(sysPath):
                        symbol.external = True
                        return
                if path.startswith(Odoo.get().stubs_dir) or \
                   path.startswith(Odoo.get().stdlib_dir):
                   symbol.external = True
                   return

    def get_paths(self):
        return []

class ConcreteSymbol(Symbol):

    def __init__(self, name, type, paths):
        super().__init__(name, type)
        self.paths = paths if isinstance(paths, list) else [paths]

    def get_paths(self):
        return self.paths

class FileSymbol(ConcreteSymbol):

    def __init__(self, name, paths):
        super().__init__(name, SymType.FILE, paths)

class PackageSymbol(ConcreteSymbol):

    def __init__(self, name, paths):
        super().__init__(name, SymType.PACKAGE, paths)

class NamespaceSymbol(ConcreteSymbol):

    def __init__(self, name, paths):
        super().__init__(name, SymType.NAMESPACE, paths)

class CompiledSymbol(ConcreteSymbol):

    def __init__(self, name, paths):
        super().__init__(name, SymType.COMPILED, paths)

class ImportSymbol(Symbol):

    def __init__(self, name):
        super().__init__(name, SymType.VARIABLE)


class FunctionSymbol(Symbol):

    def __init__(self, name, is_property):
        super().__init__(name, SymType.FUNCTION)
        self.is_property = is_property


class ClassSymbol(Symbol):

    def __init__(self, name):
        super().__init__(name, SymType.CLASS)
        self.bases = RegisteredRefSet()

    def inherits(self, symbol, checked=None):
        if not checked:
            checked = set()
        for base in self.bases:
            if base == symbol:
                return True
            if base in checked:
                return False
            checked.add(base)
            if base.inherits(symbol, checked):
                return True

    def get_context(self, args, keywords):
        return {}

    def _all_symbols_from_class(self, line=-1):
        for s in self.bases:
            for sub_s in s.all_symbols(line=-1, include_inherits=True):
                yield sub_s

    def is_inheriting_from(self, class_tree):
        from .odoo import Odoo
        for s in self.bases:
            if s.get_tree() == class_tree or s.is_inheriting_from(class_tree):
                return True
        return False

    def get_member_symbol(self, name, from_module=False, prevent_local=False, prevent_comodel=False, all=False):
        res = super().get_member_symbol(name, from_module=from_module, prevent_local=prevent_local, prevent_comodel=prevent_comodel, all=all)
        if not all and res:
            return res
        for base in self.bases:
            s = base.get_member_symbol(name, from_module=from_module, prevent_local=False, prevent_comodel=prevent_comodel, all=all)
            if s:
                if not all:
                    return s
                else:
                    for s_iter in s:
                        if s_iter not in res:
                            res.append(s_iter)
        return res

    def get_base_distance(self, base_name, level = 0):
        #for a base name, find how many levels of inheritance are needed to reach it
        if base_name == self.name:
            return level
        for base in self.bases:
            dist = base.get_base_distance(base_name, level + 1)
            if dist:
                return dist
        return 0


class SuperSymbol(Symbol):

    def __init__(self, symbol):
        """SuperSymbol is a proxy symbol that is used to handle "super()" calls. It can take multiple
        symbols that will represent the super classes, and will call them in order to respect the mro"""
        super().__init__("Super", SymType.FUNCTION)
        from .evaluation import Evaluation
        self.is_property = False
        self.eval = Evaluation()
        self.eval._symbol = RegisteredRef(self)
        self._class = symbol.get_in_parents([SymType.CLASS])

    def get_member_symbol(self, name, from_module=False, prevent_comodel=False, all=False):
        return self._class.get_member_symbol(name, from_module=from_module, prevent_local=True, prevent_comodel=prevent_comodel, all=all)

    def all_symbols(self, line=-1, include_inherits=False):
        if include_inherits:
            for sub_s in self._class._all_symbols_from_class(line=line):
                yield sub_s