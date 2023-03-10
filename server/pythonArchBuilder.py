import ast
import glob
import os
import sys
from pathlib import Path
from .constants import *
from .odoo import *
from .symbol import *
from .model import *
from .pythonUtils import *
from .importResolver import *
from lsprotocol.types import (Diagnostic,Position, Range)

class ClassContentCache():

    def __init__(self):
        self.modelName = None
        self.modelInherit = []
        self.modelInherits = []
        self.log_access = True

class PythonArchBuilder(ast.NodeVisitor):
    """The python arch builder aims to build symbols from files and directories. Only structural diagnostics
    can be thrown from here (invalid base class, etc...). Any validation diagnostics should be done byafter with
    the PythonValidator"""

    def __init__(self, ls, path, parentSymbol, subPathTree=[]):
        """Prepare an arch builder to parse the element at 'path' + subPathTree."""
        self.filePath = path
        self.symStack = [parentSymbol] # symbols we are parsing in a stack. The first element is always the parent of the current one
        self.classContentCache = []
        self.safeImport = [False] # if True, we are in a safe import (surrounded by try except)
        self.ls = ls
        self.diagnostics = []
        self.currentModule = None
        self.subPathTree = subPathTree
        self.pathTree = [] #cache tree of the current symbol

    def load_arch(self, fileInfo = False, follow_imports = True):
        """load all symbols at self.path, filtered by self.subPathTree. All dependencies (odoo modules) must have been loaded first.
        Excpected behaviour:
        On new element, not present in tree: load symbol and subsequent symbols.
        The code will follow all found import statement and try to import symbols from them too.
        On an existing symbol, the symbol will be simply returned
        """
        #TODO ensure that identical name in same file doesn't throw a reload on first one
        existing_symbol = self.symStack[-1].get_symbol([self.filePath.split(os.sep)[-1].split(".py")[0]] + self.subPathTree)
        if existing_symbol:
            return existing_symbol
        self.diagnostics = []
        if not self.filePath.endswith(".py"):
            #check if this is a package:
            if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                symbol = Symbol(self.filePath.split(os.sep)[-1], "package", self.filePath)
                if self.symStack[0].get_tree() == (["odoo", "addons"], []) and \
                    os.path.exists(os.path.join(self.filePath, "__manifest__.py")):
                     symbol.isModule = True
                self.symStack[-1].add_symbol(symbol)
                self.symStack.append(symbol)
                self.filePath = os.path.join(self.filePath, "__init__.py")
            else:
                symbol = Symbol(self.filePath.split(os.sep)[-1], "namespace", self.filePath)
                self.symStack[-1].add_symbol(symbol)
                self.symStack.append(symbol)
                return self.symStack[1]
        else:
            symbol = Symbol(self.filePath.split(os.sep)[-1].split(".py")[0], "file", self.filePath)
            self.symStack[-1].add_symbol(symbol)
            self.symStack.append(symbol)
        #parse the Python file
        self.tree = self.symStack[-1].get_tree()
        if not fileInfo:
            fileInfo = FileMgr.getFileInfo(self.filePath)
        if fileInfo["ast"]:
            self.load_symbols_from_ast(fileInfo["ast"])
            if self.symStack[-1].is_external():
                fileInfo["ast"] = None
            else:
                Odoo.get().to_validate.add(self.symStack[1])
        if self.diagnostics: #TODO not on self anymore.... take diags from fileInfo
            self.ls.publish_diagnostics(FileMgr.pathname2uri(self.filePath), self.diagnostics)
        return self.symStack[1]

    def load_symbols_from_ast(self, ast):
        moduleName = self.symStack[-1].getModule()
        if moduleName and moduleName != 'base' or moduleName in Odoo.get().modules: #TODO hack to be able to import from base when no module has been loaded yet (example services/server.py line 429 in master)
            self.currentModule = Odoo.get().modules[moduleName]
        self.visit(ast)

    def visit_Import(self, node):
        loadSymbolsFromImportStmt(self.ls, self.symStack[1], self.symStack[-1], None, 
                    [(name.name, name.asname) for name in node.names], 0, 
                    node.lineno, node.end_lineno)
        #self._resolve_import(None, [(name.name, name.asname) for name in node.names], 0, node)

    def visit_ImportFrom(self, node):
        loadSymbolsFromImportStmt(self.ls, self.symStack[1], self.symStack[-1], node.module, 
                    [(name.name, name.asname) for name in node.names], node.level, 
                    node.lineno, node.end_lineno)
        #self._resolve_import(node.module, [(name.name, name.asname) for name in node.names], node.level, node)

    def visit_Try(self, node):
        safe = False
        for handler in node.handlers:
            if not isinstance(handler.type, ast.Name):
                break
            if handler.type.id == "ImportError":
                safe = True
                break
        self.safeImport.append(safe)
        ast.NodeVisitor.generic_visit(self, node)
        self.safeImport.pop()
    
    def visit_Assign(self, node):
        assigns = self.unpack_assign(node.targets, node.value, {})
        for variable, value in assigns.items():
            if self.symStack[-1].type in ["class", "file", "package"]:
                if variable not in self.symStack[-1].symbols:
                    variable = Symbol(variable, "variable", self.filePath)
                    variable.startLine = node.lineno
                    variable.endLine = node.end_lineno
                    variable.evaluationType = PythonUtils.evaluateTypeAST(value, self.symStack[-1])
                    self.symStack[-1].add_symbol(variable)
                    if variable.name == "__all__" and self.symStack[-1].is_external():
                        # external packages often import symbols from compiled files 
                        # or with meta programmation like globals["var"] = __get_func().
                        # we don't want to handle that, so just declare __all__ content
                        # as symbols to not raise any error.
                        if variable.evaluationType and variable.evaluationType.type == "primitive":
                            for var_name in variable.evaluationType.evaluationType:
                                var = Symbol(var_name, "variable", self.filePath)
                                var.startLine = node.lineno
                                var.endLine = node.end_lineno
                                var.evaluationType = None
                                self.symStack[-1].add_symbol(var)
                else:
                    print("Warning: symbol already defined " + variable)

    def visit_FunctionDef(self, node):
        symbol = Symbol(node.name, "function", self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        self.symStack[-1].add_symbol(symbol)
        #We don't need what's inside the function?
        #self.symStack.append(symbol)
        #ast.NodeVisitor.generic_visit(self, node)
        #self.symStack.pop()
    
    def _extract_base_name(attr):
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonArchBuilder._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
        return ""

    def visit_ClassDef(self, node):
        old_sym = self.symStack[-1].symbols.pop(node.name, False)
        if old_sym:
            self.symStack[-1].localSymbols.append(old_sym)
        symbol = Symbol(node.name, "class", self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        symbol.classData = ClassData()
        self.symStack[-1].add_symbol(symbol)
        self.symStack.append(symbol)
        self.classContentCache.append(ClassContentCache())
        ast.NodeVisitor.generic_visit(self, node)
        data = self.classContentCache.pop()
        self.symStack.pop()


    def unpack_assign(self, node_targets, node_values, acc = {}):
        """ Unpack assignement to extract variables and values.
            This method will return a dictionnary that hold each variables and the set value (still in ast node)
            example: variable = variable2 = "test" (2 targets, 1 value)
            ast.Assign => {"variable": ast.Node("test"), "variable2": ast.Node("test")}
         """
        if isinstance(node_targets, ast.Attribute) or isinstance(node_targets, ast.Subscript):
            return acc
        if isinstance(node_targets, ast.Name):
            acc[node_targets.id] = node_values
            return acc
        if isinstance(node_targets, ast.Tuple) and not isinstance(node_values, ast.Tuple):
            #we can't unpack (a,b) = c as we can't unpack c here
            return acc
        for target in node_targets:
            if isinstance(target, ast.Name):
                acc[target.id] = node_values
            elif isinstance(target, ast.Tuple) and isinstance(node_values, ast.Tuple):
                if len(target.elts) != len(node_values.elts):
                    print("ERROR: unable to unpack assignement")
                    return acc
                else:
                    #TODO handle a,b = b,a
                    for nt, nv in zip(target.elts, node_values.elts):
                        self.unpack_assign(nt, nv, acc)
            elif isinstance(target, ast.Tuple):
                for elt in target.elts:
                    #We only want local variables
                    if isinstance(elt, ast.Name):
                        pass #TODO to infer this, we should be able to follow right values (func for example) and unsplit it
            else:
                pass
                # print("ERROR: unpack_assign not implemented for " + str(node_targets) + " and " + str(node_values))
        return acc
