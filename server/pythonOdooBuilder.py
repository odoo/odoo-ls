import ast
import os
from .odoo import *
from .importResolver import *
from .server import FileMgr

class PythonOdooBuilder(ast.NodeVisitor):
    """The Python Odoo Builder is the step that extracts Odoo models info for the validation.
    It represents data that are loaded and built by Odoo at loading time (model declarations, etc...)
    and that can't be used in a classic linter, due to their dynamic nature.
    This step can't be merged with Arch builder because this construction should be able to be run
    regularly like the validation, but we don't need to reload all symbols, as the file didn't change.
    In the same logic, we can't merge this step with the validation as the validation need to have all
    data coming from the simulated running odoo to work properly, so it must be done at an earlier stage.

    This class rebuild the import computation
    """

    def __init__(self, ls, symbol):
        """Prepare an odoo builder to parse the symbol"""
        self.ls = ls
        self.symStack = [symbol.get_in_parents(["file"]) or symbol] # we always load at file level
        self.diagnostics = []
        self.safeImport = [False] # if True, we are in a safe import (surrounded by try except)
        self.filePath = ""

    def load_odoo_content(self):
        self.diagnostics = []
        if self.symStack[0].odooStatus:
            return
        if self.symStack[0].type in ['namespace']:
            return
        elif self.symStack[0].type == 'package':
            self.filePath = os.path.join(self.symStack[0].paths[0], "__init__.py")
        else:
            self.filePath = self.symStack[0].paths[0]
        self.symStack[0].odooStatus = 1
        if (not Odoo.get().isLoading):
            print("Load odoo: " + self.filePath)
        fileInfo = FileMgr.getFileInfo(self.filePath)
        if not fileInfo["ast"]: #doesn"t compile or we don't want to validate it
            return
        self._load_ast(fileInfo["ast"])
        if self.diagnostics:
            fileInfo["d_odoo"] = self.diagnostics
        Odoo.get().to_validate.add(self.symStack[0])
        self.symStack[0].odooStatus = 2
        #never publish diagnostics? if a odooBuilder is involved, a validation should be too, so we can publish them together
        #FileMgr.publish_diagnostics(self.ls, fileInfo)

    def _load_ast(self, ast):
        self.visit(ast)

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

    def visit_Import(self, node):
        self._resolve_import(None, node.names, 0, node)

    def visit_ImportFrom(self, node):
        self._resolve_import(node.module, node.names, node.level, node)

    def _resolve_import(self, from_stmt, name_aliases, level, node):
        symbols = resolve_import_stmt(self.ls, self.symStack[0], self.symStack[-1], from_stmt, name_aliases, level, node.lineno, node.end_lineno)

        for node_alias, symbol, file_tree in symbols:
            name = node_alias.name
            if not symbol:
                if (file_tree + name.split("."))[0] in BUILT_IN_LIBS:
                    continue
                if not self.safeImport[-1]:
                    self.symStack[0].not_found_paths.append(file_tree + name.split("."))
                    Odoo.get().not_found_symbols.add(self.symStack[0])
                    self.diagnostics.append(Diagnostic(
                        range = Range(
                            start=Position(line=node.lineno-1, character=node.col_offset),
                            end=Position(line=node.lineno-1, character=1) if sys.version_info < (3, 8) else \
                                Position(line=node.lineno-1, character=node.end_col_offset)
                        ),
                        message = ".".join(file_tree + [name]) + " not found",
                        source = EXTENSION_NAME,
                        severity = 2
                    ))
                break
            else:
                if hasattr(node_alias, "linked_symbol"):
                    for linked_sym in node_alias.linked_symbol:
                        if name == "*":
                            symbol.arch_dependents.add(linked_sym)
                        else:
                            symbol.dependents.add(linked_sym)

    def visit_ClassDef(self, node):
        return

    def visit_Assign(self, node):
        return

    def visit_FunctionDef(self, node):
        return