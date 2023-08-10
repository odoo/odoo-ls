from _ast import AnnAssign, For
import ast
import glob
import os
import sys
from pathlib import Path
from typing import Any
from server.core.pythonArchEvalOdooHooks import PythonArchEvalOdooHooks
from server.constants import *
from server.core.evaluation import Evaluation
from server.core.odoo import *
from server.core.symbol import *
from server.core.model import *
from server.pythonUtils import *
from server.references import *
from server.core.importResolver import *
from lsprotocol.types import (Diagnostic,DiagnosticSeverity,Position, Range)


class PythonArchEval(ast.NodeVisitor):
    """The python arch eval is responsible to do the evaluation of variables extracted by the pythonArchBuilder"""

    def __init__(self, ls, symbol):
        """Prepare an arch eval to parse an element.
        To work, the symbol must have a wearef to his ast_node.
        """
        if not hasattr(symbol, "ast_node"):
            raise Exception("Symbol must have an ast_node")
        if not symbol.ast_node:
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
        self.symbol.evalStatus = 1
        self.symbol.odooStatus = 0
        self.symbol.validationStatus = 0
        ast_node = self.symbol.ast_node
        self.eval_from_ast(ast_node)
        path = self.fileSymbol.paths[0]
        if self.fileSymbol.type == SymType.PACKAGE:
            path = os.path.join(path, "__init__.py") + self.symbol.i_ext
        if self.symbol.is_external():
            FileMgr.delete_info(path)
            self.symbol.ast_node = None
        else:
            Odoo.get().add_to_init_odoo(self.symbol)
        fileInfo = FileMgr.getFileInfo(path)
        fileInfo.replace_diagnostics(BuildSteps.ARCH_EVAL, self.diagnostics)
        PythonArchEvalOdooHooks.on_file_eval(self.symbol)
        self.symbol.evalStatus = 2
        return self.symbol

    def eval_from_ast(self, ast):
        self.visit(ast)

    def visit_Import(self, node):
        self.eval_symbols_from_import_stmt(None,
                    node.names, 0, node)

    def visit_ImportFrom(self, node):
        self.eval_symbols_from_import_stmt(node.module,
                    node.names, node.level, node)

    def eval_symbols_from_import_stmt(self, from_stmt, name_aliases, level, node):
        if len(name_aliases) == 1 and name_aliases[0].name == "*":
            return
        symbols = resolve_import_stmt(self.ls, self.fileSymbol, self.symbol, from_stmt, name_aliases, level, (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset))

        for node_alias, symbol, _ in symbols:
            if not hasattr(node_alias, "symbol"): #If no symbol, the import is probably not at the top level of the file. TODO: check it?
                continue
            variable = node_alias.symbol
            if variable and symbol:
                ref = symbol.follow_ref()[0]
                old_ref = None
                while ref.eval == None and ref != old_ref:
                    old_ref = ref
                    file = ref.get_in_parents([SymType.FILE, SymType.PACKAGE])
                    if file and file.evalStatus == 0 and file in Odoo.get().rebuild_arch_eval:
                        evaluator = PythonArchEval(self.ls, file)
                        evaluator.eval_arch()
                        Odoo.get().rebuild_arch_eval.remove(file)
                    ref = ref.follow_ref()[0]
                if ref != variable: #loop detected
                    variable.ref.eval = Evaluation().eval_import(symbol)
                    variable.ref.add_dependency(symbol, BuildSteps.ARCH_EVAL, BuildSteps.ARCH)

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
            variable = hasattr(variable_name, "symbol") and variable_name.symbol and variable_name.symbol.ref or None
            if variable and variable.parent.type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                if variable:
                    variable.eval = Evaluation().evalAST(variable.value and variable.value, variable.parent)
                    if variable.eval.get_symbol():
                        variable.add_dependency(variable.eval.get_symbol(), BuildSteps.ARCH_EVAL, BuildSteps.ARCH)

    def visit_Assign(self, node):
        assigns = PythonUtils.unpack_assign(node.targets, node.value, {})
        for variable_name, value in assigns.items():
            variable = hasattr(variable_name, "symbol") and variable_name.symbol and variable_name.symbol.ref or None
            if variable and variable.parent.type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable.eval = Evaluation().evalAST(variable.value, variable.parent)
                if variable.eval.get_symbol():
                    variable.add_dependency(variable.eval.get_symbol(), BuildSteps.ARCH_EVAL, BuildSteps.ARCH)

    def visit_FunctionDef(self, node):
        if self.symbol.is_external():
            return
        ast.NodeVisitor.generic_visit(self, node)

    def _extract_base_name(attr):
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonArchEval._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
        return ""

    def _create_diagnostic_base_not_found(self, node, full_name):
        self.diagnostics.append(
            Diagnostic(
                range = Range(
                    start=Position(line=node.lineno-1, character=node.col_offset),
                    end=Position(line=node.lineno-1, character=1) if sys.version_info < (3, 8) else \
                        Position(line=node.lineno-1, character=node.end_col_offset)
                ),
                message = "Base class " + full_name + " not found",
                source = EXTENSION_NAME,
                severity= 1,
            )
        )

    def load_base_class(self, symbol, node):
        for base in node.bases:
            full_base = PythonArchEval._extract_base_name(base)
            if full_base:
                base_elements = full_base.split(".")
                iter_element = symbol.parent.inferName(base_elements[0], node.lineno)
                if not iter_element:
                    self._create_diagnostic_base_not_found(base, full_base)
                    continue
                iter_element, _ = iter_element.follow_ref()
                found = True
                for base_element in base_elements[1:]:
                    iter_element = iter_element.get_symbol([], [base_element])
                    if not iter_element:
                        found = False
                        break
                    iter_element, _ = iter_element.follow_ref()
                if not iter_element:
                    found = False
                if not found or \
                    (iter_element.type != SymType.COMPILED and \
                     not iter_element.is_external() and \
                    (iter_element.type != SymType.CLASS and not iter_element.eval)):
                    self._create_diagnostic_base_not_found(base, full_base)
                    continue
                if iter_element.type != SymType.CLASS:
                    self.diagnostics.append(
                        Diagnostic(
                            range = Range(
                                start=Position(line=base.lineno-1, character=base.col_offset),
                                end=Position(line=base.lineno-1, character=1) if sys.version_info < (3, 8) else \
                                    Position(line=base.lineno-1, character=base.end_col_offset)
                            ),
                            message = "Base class " + full_base + " is not a class",
                            source = EXTENSION_NAME,
                            severity= 1,
                        )
                    )
                    continue
                symbol.add_dependency(iter_element, BuildSteps.ARCH_EVAL, BuildSteps.ARCH)
                symbol.bases.add(iter_element)

    def visit_ClassDef(self, node):
        if not hasattr(node, "symbol") or not node.symbol:
            return
        symbol = node.symbol and node.symbol.ref
        if not symbol:
            return
        self.load_base_class(symbol, node)
        ast.NodeVisitor.generic_visit(self, node)

    def visit_For(self, node: For):
        if isinstance(node.target, ast.Name) and hasattr(node.target, "symbol"):
            symbol = node.target.symbol and node.target.symbol.ref
            if isinstance(node.iter, ast.Name):
                eval_iter_node = Evaluation().evalAST(node.iter, symbol.parent)
                if eval_iter_node.get_symbol() and eval_iter_node.get_symbol().type == SymType.CLASS:
                    iter = eval_iter_node.get_symbol().get_class_symbol("__iter__")
                    if iter and iter.eval:
                        symbol.eval = Evaluation()
                        symbol.eval.symbol = iter.eval.get_symbol_rr({"parent": eval_iter_node.get_symbol()})
                        #iter.dependents.add(variable)
                    else:
                        symbol.eval = None
        ast.NodeVisitor.generic_visit(self, node)
