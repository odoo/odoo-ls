import ast
import glob
import os
import sys
from pathlib import Path
from .constants import *
from .evaluation import Evaluation
from .odoo import *
from .symbol import *
from .model import *
from .pythonUtils import *
from .importResolver import *
from lsprotocol.types import (Diagnostic,Position, Range)


class PythonArchBuilder(ast.NodeVisitor):
    """The python arch builder aims to build symbols from files and directories. Only structural diagnostics
    can be thrown from here (invalid base class, etc...). Any validation diagnostics should be done byafter with
    the PythonValidator"""

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
                self.filePath = os.path.join(self.filePath, "__init__.py")
            self.ast_node = contentOrPath
        self.symStack = [parentSymbol] # symbols we are parsing in a stack. The first element is always the parent of the current one
        self.safeImport = [False] # if True, we are in a safe import (surrounded by try except)
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
        if (not Odoo.get().isLoading):
            print("Load arch: " + self.filePath + " " + (str(type(self.ast_node)) if self.ast_node else "") )
        if not self.ast_node: #we are parsing a whole file based on path
            existing_symbol = self.symStack[-1].get_symbol([self.filePath.split(os.sep)[-1].split(".py")[0]])
            if existing_symbol:
                return existing_symbol
            self.diagnostics = []
            if not self.filePath.endswith(".py"):
                #check if this is a package:
                if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                    symbol = Symbol(self.filePath.split(os.sep)[-1], SymType.PACKAGE, self.filePath)
                    if self.symStack[0].get_tree() == (["odoo", "addons"], []) and \
                        os.path.exists(os.path.join(self.filePath, "__manifest__.py")):
                        symbol.isModule = True
                    self.symStack[-1].add_symbol(symbol)
                    self.symStack.append(symbol)
                    self.filePath = os.path.join(self.filePath, "__init__.py")
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
            self.load_symbols_from_ast(self.ast_node or fileInfo["ast"])
            if self.symStack[-1].is_external():
                fileInfo["ast"] = None
                self.resolve__all__symbols()
            else:
                Odoo.get().to_init_odoo.add(self.symStack[-1].get_in_parents([SymType.FILE, SymType.PACKAGE]))
            if self.diagnostics: #TODO Wrong for subsymbols, but ok now as subsymbols can't raise diag :/
                fileInfo["d_arch"] = self.diagnostics
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
        #moduleName = self.symStack[-1].getModule()
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
        symbols = resolve_import_stmt(self.ls, self.symStack[-1], self.symStack[-1], from_stmt, name_aliases, level, lineno, end_lineno)

        for node_alias, symbol, _ in symbols:
            if not symbol:
                continue
            if node_alias.name != '*':
                variable = Symbol(node_alias.asname if node_alias.asname else node_alias.name, SymType.VARIABLE, self.symStack[1].paths[0])
                variable.startLine = lineno
                variable.endLine = end_lineno
                variable.eval = Evaluation().eval_import(symbol)
                variable.ast_node = weakref.ref(node)
                if hasattr(node, "linked_symbols"):
                    node.linked_symbols.add(variable)
                else:
                    node.linked_symbols = weakref.WeakSet([variable])
                self.symStack[-1].add_symbol(variable)
            else:
                allowed_names = True
                #in case of *, the symbol is the parent_symbol from which we will import all symbols
                if "__all__" in symbol.symbols:
                    all_sym = symbol.symbols["__all__"]
                    # follow ref if the current __all__ is imported
                    all_primitive_sym, _ = all_sym.follow_ref()
                    if not all_primitive_sym or not all_primitive_sym.name == "list" or not all_primitive_sym.eval.value:
                        print("debug= wrong __all__")
                        allowed_sym = True
                    else:
                        allowed_names = all_primitive_sym.eval.value
                for s in symbol.symbols.values():
                    if allowed_names == True or s.name in allowed_names:
                        variable = Symbol(s.name, SymType.VARIABLE, self.symStack[-1].paths[0])
                        variable.startLine = lineno
                        variable.endLine = end_lineno
                        variable.eval = Evaluation().eval_import(s)
                        variable.ast_node = weakref.ref(node) #TODO ref to node prevent unload to find other linked symbols
                        if hasattr(node, "linked_symbols"):
                            node.linked_symbols.add(variable)
                        else:
                            node.linked_symbols = weakref.WeakSet([variable])
                        self.symStack[-1].add_symbol(variable)

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
            if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable = Symbol(variable, SymType.VARIABLE, self.filePath)
                variable.startLine = node.lineno
                variable.endLine = node.end_lineno
                variable.ast_node = weakref.ref(node)
                variable.eval = Evaluation().evalAST(value, self.symStack[-1])
                self.symStack[-1].add_symbol(variable)
                if variable.name == "__all__" and self.symStack[-1].is_external():
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

    def visit_FunctionDef(self, node):
        symbol = Symbol(node.name, SymType.FUNCTION, self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        symbol.ast_node = weakref.ref(node)
        doc = ast.get_docstring(node)
        if doc:
            symbol.doc = Symbol("str", SymType.PRIMITIVE, self.filePath)
            symbol.doc.eval = Evaluation()
            symbol.doc.eval.value = doc
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
    
    def load_base_class(self, symbol, node):
        for base in node.bases:
            full_base = PythonArchBuilder._extract_base_name(base)
            if full_base:
                base_elements = full_base.split(".")
                iter_element = self.symStack[-1].inferName(base_elements[0], node.lineno)
                if not iter_element:
                    continue
                iter_element, _ = iter_element.follow_ref()
                found = True
                for base_element in base_elements[1:]:
                    iter_element = iter_element.get_symbol([], [base_element])
                    if not iter_element:
                        found = False
                        break
                    iter_element, _ = iter_element.follow_ref()
                if not found:
                    continue #TODO generate error?
                if iter_element.type != SymType.CLASS:
                    continue #TODO generate error?
                symbol.classData.bases.add(iter_element)

    def visit_ClassDef(self, node):
        symbol = Symbol(node.name, SymType.CLASS, self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        symbol.ast_node = weakref.ref(node)
        symbol.classData = ClassData()
        doc = ast.get_docstring(node)
        if doc:
            symbol.doc = Symbol("str", SymType.PRIMITIVE, self.filePath)
            symbol.doc.eval = Evaluation()
            symbol.doc.eval.value = doc
        #load inheritance
        self.load_base_class(symbol, node)
        self.symStack[-1].add_symbol(symbol)
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
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
