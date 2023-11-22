from _ast import AnnAssign
import ast
import os
from pathlib import Path
from typing import Any
from .python_arch_builder_odoo_hooks import PythonArchBuilderOdooHooks
from ..constants import *
from .evaluation import Evaluation
from .odoo import Odoo
from .symbol import Symbol, PackageSymbol, FileSymbol, NamespaceSymbol, ImportSymbol, ClassSymbol, FunctionSymbol
from ..python_utils import PythonUtils
from ..references import *
from .import_resolver import *
from .file_mgr import FileMgr


class PythonArchBuilder(ast.NodeVisitor):
    """The python arch builder is responsible to extract any symbol in a file or a directory and try to evaluate them.
    Of course the evaluation won't work for odoo stuff at this stage.
    There is no validation done at this step. It will only build a tree of symbols"""

    def __init__(self, ls, parentSymbol, path, ast_node=None):
        """Prepare an arch builder to parse an element.
        parentSymbol: the parent of the symbol to create
        path: path to the symbol file
        ast_node: the ast node to parse. If not provided, the file will be loaded from path
        """
        if not ast_node:
            self.ast_node = None
            self.filePath = path
        else:
            self.filePath = path
            if path.endswith("__init__.py") or path.endswith("__init__.pyi") or path.endswith("__manifest__.py"):
                self.filePath = os.path.dirname(path)
            self.ast_node = ast_node
        self.symStack = [parentSymbol] # symbols we are parsing in a stack. The first element is always the parent of the current one
        self.ls = ls
        self.diagnostics = []
        self.__all__symbols_to_add = []
        #self.currentModule = None

    def load_arch(self, require_module=False):
        """load all symbols at self.path. All dependencies (odoo modules) must have been loaded first.
        if require_module, a manifest is needed to load the arch
        Expected behaviour:
        On new element, not present in tree: load symbol and subsequent symbols.
        The code will follow all found import statement and try to import symbols from them too.
        On an existing symbol, the symbol will be simply returned
        """
        from .module import ModuleSymbol
        if DEBUG_ARCH_BUILDER:
            print("Load arch: " + self.filePath + " " + (str(type(self.ast_node)) if self.ast_node else ""))
        existing_symbol = self.symStack[-1].get_symbol([self.filePath.split(os.sep)[-1].split(".py")[0]])
        if existing_symbol:
            return existing_symbol
        self.diagnostics = []
        if not self.filePath.endswith(".py") and not self.filePath.endswith(".pyi"):
            #check if this is a package:
            if os.path.exists(os.path.join(self.filePath, "__init__.py")) or os.path.exists(os.path.join(self.filePath, "__init__.pyi")):
                if self.symStack[0].get_tree() == (["odoo", "addons"], []) and \
                    os.path.exists(os.path.join(self.filePath, "__manifest__.py")):
                    symbol = ModuleSymbol(self.ls, self.filePath)
                    symbol.load_module_info(self.ls)
                elif(not require_module):
                    symbol = PackageSymbol(self.filePath.split(os.sep)[-1], self.filePath)
                else:
                    return None
                self.symStack[-1].add_symbol(symbol)
                self.symStack.append(symbol)
                if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                    self.filePath = os.path.join(self.filePath, "__init__.py")
                else:
                    self.filePath = os.path.join(self.filePath, "__init__.pyi")
                    symbol.i_ext = "i"
            elif not require_module:
                symbol = NamespaceSymbol(self.filePath.split(os.sep)[-1], self.filePath)
                self.symStack[-1].add_symbol(symbol)
                self.symStack.append(symbol)
                return self.symStack[1]
            else:
                return None
        else:
            symbol = FileSymbol(self.filePath.split(os.sep)[-1].split(".py")[0], self.filePath)
            self.symStack[-1].add_symbol(symbol)
            self.symStack.append(symbol)
        if require_module and not isinstance(symbol, ModuleSymbol):
            return None
        #parse the Python file
        self.tree = self.symStack[-1].get_tree()
        self.symStack[-1].archStatus = 1
        fileInfo = FileMgr.get_file_info(self.filePath)
        self.symStack[-1].in_workspace = (self.symStack[-1].parent and self.symStack[-1].in_workspace) or FileMgr.is_path_in_workspace(self.ls, self.filePath)
        if fileInfo.ast:
            self.symStack[-1].ast_node = fileInfo.ast
            #Odoo.get().rebuild_arch.remove(self.symStack[-1])
            self.load_symbols_from_ast(self.ast_node or fileInfo.ast)
            if self.symStack[-1].is_external():
                self.resolve__all__symbols()
            fileInfo.replace_diagnostics(BuildSteps.ARCH, self.diagnostics)
            if not fileInfo.diagnostics[BuildSteps.SYNTAX] and not self.diagnostics:
                Odoo.get().add_to_arch_eval(self.symStack[-1].get_in_parents([SymType.FILE, SymType.PACKAGE]))
            elif self.symStack[-1].in_workspace:
                fileInfo.publish_diagnostics(self.ls)
        if self.filePath.endswith("__init__.py"):
            PythonArchBuilderOdooHooks.on_package_declaration(self.symStack[-1])
        if self.symStack[-1].type == SymType.FILE:
            PythonArchBuilderOdooHooks.on_file_declaration(self.symStack[-1])
        #print("END arch: " + self.filePath + " " + (str(type(self.ast_node)) if self.ast_node else "") )
        self.symStack[-1].archStatus = 2
        return self.symStack[-1]

    def resolve__all__symbols(self):
        #at the end, add all symbols from __all__ statement that couldn't be loaded (because of dynamical import)
        #Mainly for external packages
        for symbol in self.__all__symbols_to_add:
            if symbol.name not in self.symStack[-1].symbols.keys():
                self.symStack[-1].add_symbol(symbol)

    def load_symbols_from_ast(self, ast):
        #moduleName = self.symStack[-1].get_module_sym()
        #if moduleName and moduleName != 'base' or moduleName in Odoo.get().modules: #TODO hack to be able to import from base when no module has been loaded yet (example services/server.py line 429 in master)
        #    self.currentModule = Odoo.get().modules[moduleName]
        self.visit(ast)

    def visit_Import(self, node):
        self.create_local_symbols_from_import_stmt(None,
                    node.names, 0, node)

    def visit_ImportFrom(self, node):
        self.create_local_symbols_from_import_stmt(node.module,
                node.names, node.level, node)

    def create_local_symbols_from_import_stmt(self, from_stmt, name_aliases, level, node):
        for import_name in name_aliases:
            if import_name.name == '*':
                if len(self.symStack) != 2: # only at top level. we can't follow the import at arch level
                    continue
                symbols = resolve_import_stmt(self.ls, self.symStack[-1], self.symStack[-1], from_stmt, name_aliases, level, (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset))
                _, found, symbol, tree = symbols[0] #unpack
                if not found:
                    Odoo.get().not_found_symbols.add(self.symStack[1])
                    self.symStack[1].not_found_paths.append((BuildSteps.ARCH, tree))
                    continue
                allowed_names = True
                #in case of *, the symbol is the parent_symbol from which we will import all symbols
                if "__all__" in symbol.symbols:
                    all_sym = symbol.symbols["__all__"]
                    # follow ref if the current __all__ is imported
                    all_primitive_sym, _ = all_sym.follow_ref()
                    if not all_primitive_sym or not all_primitive_sym.name in ["list", "tuple"] or not all_primitive_sym.value:
                        print("debug= wrong __all__")
                    else:
                        allowed_names = list(all_primitive_sym.value)
                fileSymbol = self.symStack[1]
                for s in symbol.symbols.values():
                    if allowed_names == True or s.name in allowed_names:
                        variable = ImportSymbol(s.name)
                        variable.start_pos = (node.lineno, node.col_offset)
                        variable.end_pos = (node.end_lineno, node.end_col_offset)
                        variable.eval = Evaluation().eval_import(s)
                        eval_sym = variable.eval.get_symbol()
                        if eval_sym:
                            fileSymbol.add_dependency(eval_sym, BuildSteps.ARCH, BuildSteps.ARCH)
                        self.symStack[-1].add_symbol(variable)
            else:
                variable = ImportSymbol(import_name.asname if import_name.asname else import_name.name.split(".")[0])
                variable.start_pos, variable.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
                import_name.symbol = RegisteredRef(variable)
                self.symStack[-1].add_symbol(variable)

    def visit_AnnAssign(self, node: AnnAssign) -> Any:
        assigns = PythonUtils.unpack_assign(node.target, node.value, {})
        for variable_name, value in assigns.items():
            if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable = Symbol(variable_name.id, SymType.VARIABLE)
                variable.start_pos, variable.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
                if value:
                    variable.value = value
                variable_name.symbol = RegisteredRef(variable)
                self.symStack[-1].add_symbol(variable)

    def visit_Assign(self, node):
        assigns = PythonUtils.unpack_assign(node.targets, node.value, {})
        for variable_name, value in assigns.items():
            if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable = Symbol(variable_name.id, SymType.VARIABLE)
                variable.start_pos, variable.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
                variable.value = value
                self.symStack[-1].add_symbol(variable)
                if variable.name == "__all__":
                    variable.eval = Evaluation().evalAST(variable.value and variable.value, variable.parent)
                    if variable.eval.get_symbol():
                        file_symbol = self.symStack[1]
                        file_symbol.add_dependency(variable.eval.get_symbol(), BuildSteps.ARCH, BuildSteps.ARCH)

                    if self.symStack[-1].is_external():
                        # external packages often import symbols from compiled files
                        # or with meta programmation like globals["var"] = __get_func().
                        # we don't want to handle that, so just declare __all__ content
                        # as symbols to not raise any error.
                        evaluation = variable.eval
                        if evaluation and evaluation.get_symbol() and evaluation.get_symbol().type == SymType.PRIMITIVE:
                            for var_name in evaluation.get_symbol().value:
                                var = Symbol(var_name, SymType.VARIABLE)
                                var.start_pos, var.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
                                var.eval = None
                                self.__all__symbols_to_add.append(var)
                else:
                    variable_name.symbol = RegisteredRef(variable)

    def visit_FunctionDef(self, node):
        #test if static:
        is_static = False
        is_property = False
        for decorator in node.decorator_list:
            if isinstance(decorator, ast.Name) and decorator.id == "staticmethod":
                is_static = True
            if isinstance(decorator, ast.Name) and decorator.id == "property":
                is_property = True
        symbol = FunctionSymbol(node.name, is_property)
        symbol.start_pos, symbol.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
        doc = ast.get_docstring(node)
        if doc:
            symbol.doc = Symbol("str", SymType.PRIMITIVE)
            symbol.doc.value = doc
        if not is_static and node.args:
            class_sym = self.symStack[-1]
            if class_sym and class_sym.type == SymType.CLASS and node.args.args:
                self_name = node.args.args[0].arg
                self_sym = Symbol(self_name, SymType.VARIABLE)
                self_sym.start_pos, self_sym.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
                self_sym.eval = Evaluation()
                self_sym.eval.symbol = RegisteredRef(class_sym) #no dep required here
                symbol.add_symbol(self_sym)
        self.symStack[-1].add_symbol(symbol)

    def visit_ClassDef(self, node):
        symbol = ClassSymbol(node.name)
        symbol.start_pos, symbol.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
        node.symbol = RegisteredRef(symbol)
        doc = ast.get_docstring(node)
        if doc:
            symbol.doc = Symbol("str", SymType.PRIMITIVE)
            symbol.doc.value = doc
        self.symStack[-1].add_symbol(symbol)
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symStack.pop()
        PythonArchBuilderOdooHooks.on_class_declaration(symbol)

    def visit_For(self, node):
        if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE, SymType.FUNCTION]:
            if isinstance(node.target, ast.Name): #do not handle tuples for now
                variable = Symbol(node.target.id, SymType.VARIABLE)
                variable.start_pos, variable.end_pos = (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset)
                node.target.symbol = RegisteredRef(variable)
                self.symStack[-1].add_symbol(variable)
        ast.NodeVisitor.generic_visit(self, node)
