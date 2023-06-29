from _ast import AnnAssign, For
import ast
import glob
import os
import sys
from pathlib import Path
from typing import Any
from server.core.pythonArchBuilderOdooHooks import PythonArchBuilderOdooHooks
from server.constants import *
from server.core.evaluation import Evaluation
from server.core.odoo import *
from server.core.symbol import *
from server.core.model import *
from server.pythonUtils import *
from server.core.importResolver import *
from lsprotocol.types import (Diagnostic,Position, Range)


class PythonArchBuilder(ast.NodeVisitor):
    """The python arch builder is responsible to extract any symbol in a file or a directory and try to evaluate them.
    Of course the evaluation won't work for odoo stuff at this stage.
    There is no validation done at this step. It will only build a tree of symbols"""

    def __init__(self, ls, parentSymbol, contentOrPath):
        """Prepare an arch builder to parse an element.
        if contentOrPath is string, it must be a path to a file/direcotry/package.
        If not, it must be a ref to an ast node that contains the element to parse
        """
        if isinstance(contentOrPath, str):
            self.ast_node = None
            self.filePath = contentOrPath
        else:
            parent_file = parentSymbol.get_in_parents([SymType.FILE, SymType.PACKAGE])
            self.filePath = parent_file.paths[0]
            if parent_file.type == SymType.PACKAGE:
                self.filePath = os.path.join(self.filePath, "__init__.py" + parent_file.i_ext)
            self.ast_node = contentOrPath
        self.symStack = [parentSymbol] # symbols we are parsing in a stack. The first element is always the parent of the current one
        self.ls = ls
        self.diagnostics = []
        self.__all__symbols_to_add = []
        #self.currentModule = None

    def load_arch(self):
        """load all symbols at self.path. All dependencies (odoo modules) must have been loaded first.
        Excpected behaviour:
        On new element, not present in tree: load symbol and subsequent symbols.
        The code will follow all found import statement and try to import symbols from them too.
        On an existing symbol, the symbol will be simply returned
        """
        if DEBUG_ARCH_BUILDER:
            print("Load arch: " + self.filePath + " " + (str(type(self.ast_node)) if self.ast_node else "") )
        if not self.ast_node: #we are parsing a whole file based on path
            existing_symbol = self.symStack[-1].get_symbol([self.filePath.split(os.sep)[-1].split(".py")[0]])
            if existing_symbol:
                return existing_symbol
            self.diagnostics = []
            if not self.filePath.endswith(".py") and not self.filePath.endswith(".pyi"):
                #check if this is a package:
                if os.path.exists(os.path.join(self.filePath, "__init__.py")) or os.path.exists(os.path.join(self.filePath, "__init__.pyi")):
                    symbol = Symbol(self.filePath.split(os.sep)[-1], SymType.PACKAGE, self.filePath)
                    if self.symStack[0].get_tree() == (["odoo", "addons"], []) and \
                        os.path.exists(os.path.join(self.filePath, "__manifest__.py")):
                        symbol.isModule = True
                    self.symStack[-1].add_symbol(symbol)
                    self.symStack.append(symbol)
                    if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                        self.filePath = os.path.join(self.filePath, "__init__.py")
                    else:
                        self.filePath = os.path.join(self.filePath, "__init__.pyi")
                        symbol.i_ext = "i"
                else:
                    symbol = Symbol(self.filePath.split(os.sep)[-1], SymType.NAMESPACE, self.filePath)
                    self.symStack[-1].add_symbol(symbol)
                    self.symStack.append(symbol)
                    return self.symStack[1]
            else:
                symbol = Symbol(self.filePath.split(os.sep)[-1].split(".py")[0], SymType.FILE, self.filePath)
                self.symStack[-1].add_symbol(symbol)
                self.symStack.append(symbol)
        #parse the Python file
        self.tree = self.symStack[-1].get_tree()
        fileInfo = FileMgr.getFileInfo(self.filePath)
        if fileInfo["ast"]:
            self.symStack[-1].ast_node = weakref.ref(fileInfo["ast"])
            #Odoo.get().rebuild_arch.remove(self.symStack[-1])
            self.load_symbols_from_ast(self.ast_node or fileInfo["ast"])
            if self.symStack[-1].is_external():
                fileInfo["ast"] = None
                self.resolve__all__symbols()
            else:
                Odoo.get().add_to_arch_eval(self.symStack[-1].get_in_parents([SymType.FILE, SymType.PACKAGE]))
            if self.diagnostics: #TODO Wrong for subsymbols, but ok now as subsymbols can't raise diag :/
                fileInfo["d_arch"] = self.diagnostics
        if self.filePath.endswith("__init__.py"):
            PythonArchBuilderOdooHooks.on_module_declaration(self.symStack[-1])
        FileMgr.publish_diagnostics(self.ls, fileInfo)
        #print("END arch: " + self.filePath + " " + (str(type(self.ast_node)) if self.ast_node else "") )
        return self.symStack[-1]

    def resolve__all__symbols(self):
        #at the end, add all symbols from __all__ statement that couldn't be loaded (because of dynamical import)
        #Mainly for external packages
        for symbol in self.__all__symbols_to_add:
            if symbol not in self.symStack[-1].symbols:
                self.symStack[-1].add_symbol(symbol)

    def load_symbols_from_ast(self, ast):
        #moduleName = self.symStack[-1].get_module()
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
        lineno = node.lineno
        end_lineno = node.end_lineno

        for import_name in name_aliases:
            if import_name.name == '*':
                symbols = resolve_import_stmt(self.ls, self.symStack[-1], self.symStack[-1], from_stmt, name_aliases, level, lineno, end_lineno)
                _, symbol, _ = symbols[0] #unpack
                if not symbol:
                    continue
                allowed_names = True
                #in case of *, the symbol is the parent_symbol from which we will import all symbols
                if "__all__" in symbol.symbols:
                    all_sym = symbol.symbols["__all__"]
                    # follow ref if the current __all__ is imported
                    all_primitive_sym, _ = all_sym.follow_ref()
                    if not all_primitive_sym or not all_primitive_sym.name in ["list", "tuple"] or not all_primitive_sym.eval.value:
                        print("debug= wrong __all__")
                    else:
                        allowed_names = list(all_primitive_sym.eval.value)
                fileSymbol = self.symStack[1]
                for s in symbol.symbols.values():
                    if allowed_names == True or s.name in allowed_names:
                        variable = Symbol(s.name, SymType.VARIABLE, self.symStack[-1].paths[0])
                        variable.startLine = lineno
                        variable.endLine = end_lineno
                        variable.eval = Evaluation().eval_import(s)
                        variable.ast_node = weakref.ref(node) #TODO ref to node prevent unload to find other linked symbols
                        #if hasattr(node, "linked_symbols"):
                        #    node.linked_symbols.add(variable)
                        #else:
                        #    node.linked_symbols = weakref.WeakSet([variable])
                        eval_sym = variable.eval.getSymbol()
                        if eval_sym:
                            eval_sym.get_in_parents([SymType.FILE, SymType.PACKAGE]).arch_dependents[BuildSteps.ARCH].add(fileSymbol) #put file as dependent, to lower memory usage, as the rebuild is done at file level
                        self.symStack[-1].add_symbol(variable)
            else:
                variable = Symbol(import_name.asname if import_name.asname else import_name.name, SymType.VARIABLE, self.symStack[1].paths[0])
                variable.startLine = lineno
                variable.endLine = end_lineno
                import_name.symbol = weakref.ref(variable)
                variable.ast_node = weakref.ref(node)
                self.symStack[-1].add_symbol(variable)
    
    def visit_AnnAssign(self, node: AnnAssign) -> Any:
        assigns = PythonUtils.unpack_assign(node.target, node.value, {})
        for variable_name, value in assigns.items():
            if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable = Symbol(variable_name.id, SymType.VARIABLE, self.filePath)
                variable.startLine = node.lineno
                variable.endLine = node.end_lineno
                variable.ast_node = weakref.ref(node)
                if value:
                    variable.value = weakref.ref(value)
                variable_name.symbol = weakref.ref(variable)
                self.symStack[-1].add_symbol(variable)

    def visit_Assign(self, node):
        assigns = PythonUtils.unpack_assign(node.targets, node.value, {})
        for variable_name, value in assigns.items():
            if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable = Symbol(variable_name.id, SymType.VARIABLE, self.filePath)
                variable.startLine = node.lineno
                variable.endLine = node.end_lineno
                variable.ast_node = weakref.ref(node)
                variable.value = weakref.ref(value)
                self.symStack[-1].add_symbol(variable)
                if variable.name == "__all__":
                    variable.eval = Evaluation().evalAST(variable.value and variable.value(), variable.parent)
                    if variable.eval.getSymbol():
                        eval_file_symbol = variable.eval.getSymbol().get_in_parents([SymType.FILE, SymType.PACKAGE])
                        file_symbol = self.symStack[1]
                        if eval_file_symbol != file_symbol:
                            variable.eval.getSymbol().arch_dependents[BuildSteps.ARCH].add(file_symbol)
                        
                    if self.symStack[-1].is_external():
                        # external packages often import symbols from compiled files 
                        # or with meta programmation like globals["var"] = __get_func().
                        # we don't want to handle that, so just declare __all__ content
                        # as symbols to not raise any error.
                        evaluation = variable.eval
                        if evaluation and evaluation.getSymbol() and evaluation.getSymbol().type == SymType.PRIMITIVE:
                            for var_name in evaluation.getSymbol().eval.value:
                                var = Symbol(var_name, SymType.VARIABLE, self.filePath)
                                var.startLine = node.lineno
                                var.endLine = node.end_lineno
                                var.eval = None
                                self.__all__symbols_to_add.append(var)
                else:
                    variable_name.symbol = weakref.ref(variable)

    def visit_FunctionDef(self, node):
        #test if static:
        is_static = False
        for decorator in node.decorator_list:
            if isinstance(decorator, ast.Name) and decorator.id == "staticmethod":
                is_static = True
                break
        symbol = Symbol(node.name, SymType.FUNCTION, self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        symbol.ast_node = weakref.ref(node)
        doc = ast.get_docstring(node)
        if doc:
            symbol.doc = Symbol("str", SymType.PRIMITIVE, self.filePath)
            symbol.doc.eval = Evaluation()
            symbol.doc.eval.value = doc
        if not is_static and node.args:
            class_sym = self.symStack[-1]
            if class_sym and class_sym.type == SymType.CLASS and node.args.args:
                self_name = node.args.args[0].arg
                self_sym = Symbol(self_name, SymType.VARIABLE, self.filePath)
                self_sym.startLine = node.lineno
                self_sym.endLine = node.end_lineno
                self_sym.ast_node = weakref.ref(node)
                self_sym.eval = Evaluation()
                self_sym.eval.symbol = weakref.ref(class_sym) #no dep required here
                symbol.add_symbol(self_sym)
        self.symStack[-1].add_symbol(symbol)
        #We don't need what's inside the function?
        if self.symStack[-1].is_external():
            return
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symStack.pop()

    def visit_ClassDef(self, node):
        symbol = Symbol(node.name, SymType.CLASS, self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        node.symbol = weakref.ref(symbol)
        symbol.ast_node = weakref.ref(node)
        symbol.classData = ClassData()
        doc = ast.get_docstring(node)
        if doc:
            symbol.doc = Symbol("str", SymType.PRIMITIVE, self.filePath)
            symbol.doc.eval = Evaluation()
            symbol.doc.eval.value = doc
        self.symStack[-1].add_symbol(symbol)
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symStack.pop()
        PythonArchBuilderOdooHooks.on_class_declaration(symbol)

    def visit_For(self, node):
        if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE, SymType.FUNCTION]:
            if isinstance(node.target, ast.Name): #do not handle tuples for now
                variable = Symbol(node.target.id, SymType.VARIABLE, self.filePath)
                variable.startLine = node.lineno
                variable.endLine = node.end_lineno
                variable.ast_node = weakref.ref(node)
                #TODO move to arch_eval
                if isinstance(node.iter, ast.Name):
                    eval_iter_node = Evaluation().evalAST(node.iter, self.symStack[-1])
                    if eval_iter_node.getSymbol() and eval_iter_node.getSymbol().type == SymType.CLASS:
                        iter = eval_iter_node.getSymbol().get_class_symbol("__iter__")
                        if iter and iter.eval:
                            variable.eval = Evaluation()
                            variable.eval.symbol = iter.eval.get_symbol_wr({"self": eval_iter_node.getSymbol()})
                            #iter.dependents.add(variable)
                        else:
                            variable.eval = None
                self.symStack[-1].add_symbol(variable)
        ast.NodeVisitor.generic_visit(self, node)
