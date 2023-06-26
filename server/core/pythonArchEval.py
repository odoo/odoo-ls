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


class PythonArchEval(ast.NodeVisitor):
    """The python arch eval is responsible to do the evaluation of variables extracted by the pythonArchBuilder"""

    def __init__(self, ls, symbol):
        """Prepare an arch eval to parse an element.
        To work, the symbol must have a wearef to his ast_node.
        """
        if not hasattr(symbol, "ast_node"):
            raise Exception("Symbol must have an ast_node")
        if not symbol.ast_node():
            raise Exception("Symbol must have a valid ast_node")
        self.symbol = symbol
        self.fileSymbol = self.symbol.get_in_parents([SymType.FILE, SymType.PACKAGE])
        self.ls = ls
        self.diagnostics = []
        #self.currentModule = None

    def eval_arch(self):
        """pass through the ast to find symbols to evaluate"""
        if DEBUG_ARCH_EVAL:
            print("Eval arch: " + self.fileSymbol.paths[0])
        ast_node = self.symbol.ast_node()
        self.eval_from_ast(ast_node)
        if not self.symbol.is_external():
            Odoo.get().add_to_init_odoo(self.symbol)
        path = self.fileSymbol.paths[0]
        if self.fileSymbol.type == SymType.PACKAGE:
            path = os.path.join(path, "__init__.py")
        fileInfo = FileMgr.getFileInfo(path)
        fileInfo["d_arch_eval"] = self.diagnostics
        #if self.filePath.endswith("__init__.py"): #TODO update hooks
        #    PythonArchBuilderOdooHooks.on_module_declaration(self.symStack[-1])
        FileMgr.publish_diagnostics(self.ls, fileInfo)
        #print("END arch: " + self.filePath + " " + (str(type(self.ast_node)) if self.ast_node else "") )
        return self.symbol

    def resolve__all__symbols(self):
        #at the end, add all symbols from __all__ statement that couldn't be loaded (because of dynamical import)
        #Mainly for external packages
        for symbol in self.__all__symbols_to_add:
            if symbol not in self.symStack[-1].symbols:
                self.symStack[-1].add_symbol(symbol)

    def eval_from_ast(self, ast):
        self.visit(ast)

    def visit_Import(self, node):
        self.eval_symbols_from_import_stmt(None, 
                    node.names, 0, node)

    def visit_ImportFrom(self, node):
        self.eval_symbols_from_import_stmt(node.module, 
                    node.names, node.level, node)

    def eval_symbols_from_import_stmt(self, from_stmt, name_aliases, level, node):
        lineno = node.lineno
        end_lineno = node.end_lineno
        if len(name_aliases) == 1 and name_aliases[0].name == "*":
            return
        symbols = resolve_import_stmt(self.ls, self.fileSymbol, self.symbol, from_stmt, name_aliases, level, lineno, end_lineno)

        for node_alias, symbol, _ in symbols:
            if not hasattr(node_alias, "symbol"):
                print("Node has no symbol. An error occured")
                continue
            variable = node_alias.symbol
            if variable and symbol:
                variable().eval = Evaluation().eval_import(symbol)
                symbol.eval_dependents[BuildSteps.ARCH_EVAL].add(variable().get_in_parents([SymType.FILE, SymType.PACKAGE]))

    def visit_Try(self, node):
        return
        safe = False
        for handler in node.handlers:
            if not isinstance(handler.type, ast.Name):
                continue
            if handler.type.id == "ImportError":
                safe = True
                break
        self.safeImport.append(safe)
        ast.NodeVisitor.generic_visit(self, node)
        self.safeImport.pop()
    
    def visit_AnnAssign(self, node: AnnAssign) -> Any:
        assigns = PythonUtils.unpack_assign(node.target, node.value, {})
        for variable_name, value in assigns.items():
            variable = hasattr(variable_name, "symbol") and variable_name.symbol and variable_name.symbol() or None
            if variable and variable.parent.type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                if variable:
                    variable.eval = Evaluation().evalAST(variable.value and variable.value(), variable.parent)
                    if variable.eval.getSymbol():
                        variable.eval.getSymbol().eval_dependents[BuildSteps.ARCH_EVAL].add(variable.get_in_parents([SymType.FILE, SymType.PACKAGE]))

    def visit_Assign(self, node):
        assigns = PythonUtils.unpack_assign(node.targets, node.value, {})
        for variable_name, value in assigns.items():
            variable = hasattr(variable_name, "symbol") and variable_name.symbol and variable_name.symbol() or None
            if variable and variable.parent.type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                if variable:
                    variable.eval = Evaluation().evalAST(variable.value and variable.value(), variable.parent)
                    if variable.eval.getSymbol():
                        variable.eval.getSymbol().eval_dependents[BuildSteps.ARCH_EVAL].add(variable.get_in_parents([SymType.FILE, SymType.PACKAGE]))

    def visit_FunctionDef(self, node):
        return
        #We don't need what's inside the function?
        if self.symStack[-1].is_external():
            return
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symStack.pop()
    
    def _extract_base_name(attr):
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonArchEval._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
        return ""
    
    def load_base_class(self, symbol, node):
        for base in node.bases:
            full_base = PythonArchEval._extract_base_name(base)
            if full_base:
                base_elements = full_base.split(".")
                iter_element = symbol.parent.inferName(base_elements[0], node.lineno)
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
                    continue #TODO generate error? add to unresolved
                if iter_element.type != SymType.CLASS:
                    continue #TODO generate error?
                iter_element.eval_dependents[BuildSteps.ARCH_EVAL].add(symbol.get_in_parents([SymType.FILE, SymType.PACKAGE]))
                symbol.classData.bases.add(iter_element)

    def visit_ClassDef(self, node):
        if not hasattr(node, "symbol") or not node.symbol:
            return
        symbol = node.symbol()
        if not symbol:
            return
        self.load_base_class(symbol, node)
        ast.NodeVisitor.generic_visit(self, node)
        

