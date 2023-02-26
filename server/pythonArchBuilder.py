import ast
import os
import sys
from pathlib import Path
from .constants import *
from .odoo import *
from .symbol import *
from .model import *
from .pythonUtils import *
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

    def __init__(self, ls, path, parentSymbol, subPathTree=[], importMode=False):
        """Prepare an arch builder to parse the element at 'path' + subPathTree.
        if importMode, the symbol will be not built if they already exists. It is used to check that a module/file has
        been imported or import it without reconstructing it multiple time"""
        self.filePath = path
        self.symStack = [parentSymbol] # symbols we are parsing in a stack. The first element is always the parent of the current one
        self.classContentCache = []
        self.safeImport = [False] # if True, we are in a safe import (surrounded by try except)
        self.ls = ls
        self.diagnostics = []
        self.currentModule = None
        self.subPathTree = subPathTree
        self.importMode = importMode
        self.pathTree = [] #cache tree of the current symbol

    def load_arch(self, content = False, version = 1):
        """load all symbols at self.path, filtered by self.subPathTree. All dependencies (odoo modules) must have been loaded first.
        Excpected behaviour:
        On new element, not present in tree: load symbol and subsequent symbols.
        The code will follow all found import statement and try to import symbols from them too.
        On an existing symbol, the symbol will be unloaded and dependents flagged as to revalidate.
        The symbol unloaded is then reloaded, but imports are not followed
        """
        #TODO ensure that identical name in same file doesn't throw a reload on first one
        existing_symbol = self.symStack[-1].get_symbol([self.filePath.split(os.sep)[-1].split(".py")[0]] + self.subPathTree)
        if existing_symbol:
            if self.importMode:
                return existing_symbol
            existing_symbol.unload()
        self.diagnostics = []
        if not self.filePath.endswith(".py"):
            #check if this is a package:
            if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                symbol = Symbol(self.filePath.split(os.sep)[-1], "package", self.filePath)
                if self.symStack[0].get_tree() == ["odoo", "addons"] and \
                    os.path.exists(os.path.join(self.filePath, "__manifest__.py")):
                    symbol.isModule = True
                self.symStack[-1].add_symbol([], symbol)
                self.symStack.append(symbol)
                self.filePath = os.path.join(self.filePath, "__init__.py")
            else:
                symbol = Symbol(self.filePath.split(os.sep)[-1], "namespace", self.filePath)
                self.symStack[-1].add_symbol([], symbol)
                self.symStack.append(symbol)
                return self.symStack[1]
        else:
            symbol = Symbol(self.filePath.split(os.sep)[-1].split(".py")[0], "file", self.filePath)
            self.symStack[-1].add_symbol([], symbol)
            self.symStack.append(symbol)
        #parse the Python file
        self.tree = self.symStack[-1].get_tree()
        fileInfo = FileMgr.getFileInfo(self.filePath, content, version)
        if fileInfo["ast"]:
            self.load_symbols_from_ast(fileInfo["ast"])
        if self.diagnostics: #TODO not on self anymore.... take diags from fileInfo
            self.ls.publish_diagnostics(FileMgr.pathname2uri(self.filePath), self.diagnostics)
        return self.symStack[1]

    def load_symbols_from_ast(self, ast):
        moduleName = self.symStack[-1].getModule()
        if moduleName and moduleName != 'base' or moduleName in Odoo.get().modules: #TODO hack to be able to import from base when no module has been loaded yet (example services/server.py line 429 in master)
            self.currentModule = Odoo.get().modules[moduleName]
        self.visit(ast)

    def visit_Import(self, node):
        self._resolve_import(None, [(name.name, name.asname) for name in node.names], 0, node)

    def visit_ImportFrom(self, node):
        self._resolve_import(node.module, [(name.name, name.asname) for name in node.names], node.level, node)

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


    def _resolve_import(self, from_stmt, names, level, node):
        packages = []
        if level != 0:
            if level > len(Path(self.filePath).parts):
                print("ERROR: level is too big ! The current path doesn't have enough parents")
                return
            if self.symStack[1].type == "package":
                #as we are at directory level, not init.py
                level -= 1
            if level == 0:
                packages = self.symStack[1].get_tree()
            else:
                packages = self.symStack[1].get_tree()[:-level]

        for name, asname in names:
            packages_copy = packages[:]
            elements = from_stmt.split(".") if from_stmt != None else []
            elements += name.split(".") if name != None else []

            for element in elements:
                if element != '*':
                    current_symbol = Odoo.get().symbols.get_symbol(packages_copy)
                    if not current_symbol:
                        if packages_copy and packages_copy[0] == "odoo":
                            pass#print(packages_copy)
                        break
                    next_step_symbols = Odoo.get().symbols.get_symbol(packages_copy + [element])
                    if not next_step_symbols:
                        symbol_paths = current_symbol.paths if current_symbol else []
                        for path in symbol_paths:
                            full_path = path + os.sep + element
                            if os.path.isdir(full_path):
                                if current_symbol.get_tree() == ["odoo", "addons"]:
                                    module = self.symStack[-1].getModule()
                                    if module and not Odoo.get().modules[module].is_in_deps(element):
                                        if not any(self.safeImport):
                                            self.diagnostics.append(Diagnostic(
                                                range = Range(
                                                    start=Position(line=node.lineno-1, character=node.col_offset),
                                                    end=Position(line=node.lineno-1, character=1) if sys.version_info < (3, 8) else \
                                                        Position(line=node.lineno-1, character=node.end_col_offset)
                                                ),
                                                message = element + " has not been loaded. It should be in dependencies of " + module,
                                                source = EXTENSION_NAME
                                            ))
                                        #We return in any case here. the module is not in dependencies, so it could be not
                                        #loaded and load it now could break the import system. example mail_thread.py:L2041
                                        #If the import is really needed for inferencer, it will be done in pythonValidator, not here
                                        return
                                    if not module:
                                        """If we are searching for a odoo.addons.* element, skip it if we are not in a module.
                                        It means we are in a file like odoo/*, and modules are not loaded yet."""
                                        return
                                parser = PythonArchBuilder(self.ls, full_path, current_symbol, importMode=True)
                                parser.load_arch()
                                break
                            elif os.path.isfile(full_path + ".py"):
                                parser = PythonArchBuilder(self.ls, full_path + ".py", current_symbol, importMode=True)
                                parser.load_arch()
                                break
                    packages_copy += [element]
            if elements[-1] != '*':
                sym = Odoo.get().symbols.get_symbol(packages_copy)
                if sym and level > 0:
                    if self.symStack[-1].get_tree() not in sym.dependents:
                        sym.dependents.append(self.symStack[-1].get_tree())
    
    def visit_Assign(self, node):
        assigns = self.unpack_assign(node.targets, node.value, {})
        for variable, value in assigns.items():
            if self.symStack[-1].type in ["class", "file"]:
                if variable not in self.symStack[-1].symbols:
                    variable = Symbol(variable, "variable", self.filePath)
                    variable.startLine = node.lineno
                    variable.endLine = node.end_lineno
                    variable.evaluationType = PythonUtils.evaluateTypeAST(value, self.symStack[-1])
                    self.symStack[-1].add_symbol([], variable)
                else:
                    print("Warning: symbol already defined")

    def visit_FunctionDef(self, node):
        symbol = Symbol(node.name, "function", self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        self.symStack[-1].add_symbol([], symbol)
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symStack.pop()
    
    def _extract_base_name(attr):
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonArchBuilder._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
        return ""

    def visit_ClassDef(self, node):
        if node.name not in self.symStack[-1].symbols: #TODO ouch, this is the last that should be kept...
            symbol = Symbol(node.name, "class", self.filePath)
            symbol.evaluationType = symbol
            symbol.startLine = node.lineno
            symbol.endLine = node.end_lineno
            symbol.classData = ClassData()
            self.symStack[-1].add_symbol([], symbol)
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
        for target in node_targets:
            if isinstance(target, ast.Name):
                acc[target.id] = node_values
            elif isinstance(target, ast.Tuple) and isinstance(node_values, ast.Tuple):
                if len(target.elts) != len(node_values.elts):
                    print("ERROR: unable to unpack assignement")
                    return acc
                else:
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
