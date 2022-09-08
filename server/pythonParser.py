import ast
import os
import sys
from pathlib import Path
from server.odooBase import Symbol, OdooBase

class PythonParser(ast.NodeVisitor):
    """This class read a file and extract all relevant data. Classes, functions and models are stored in odooBase.
    It imports also all needed imports and store relevant data from them into odooBase."""

    parsed = {}

    def __init__(self, ls, path, symbol):
        self.filePath = path
        self.symbol = symbol # symbol we are parsing
        self.mode = 'symbols'
        self.ls = ls
        #local namespace and corresponding aliases
        # { name: 
        #      {
        #       "real_name": real_name,
        #       "packages": list of packages
        #      }
        # }
        # example:
        # from odoo.addons.account import Account
        # self.localAliases = {
        #    "Account": {
        #       "real_name": "account",
        #       "packages": ["odoo", "addons"]
        # }
        self.localAliases = {}
        self.reset()

    def reset(self):
        """ reset the data from last parsing """
        self.local_symbols = {}

        #current parsing
        self.currentClass = None

    def getImportPaths(self):
        """ Return the import statements in the file """
        pass

    def parse(self):
        """ Parse the file to extract relevant informations """
        self.reset()
        OdooBase.get().files[self.filePath] = {"symbols": [], "imports": []}
        try:
            with open(self.filePath, "rb") as f:
                content = f.read()
            tree = ast.parse(content, self.filePath)
            self.visit(tree)
        except SyntaxError as e:
            self.ls.publish_diagnostics(self.filePath, [Diagnostic(
                range = Range(
                    start=Position(line=e.lineno, character=e.offset),
                    end=Position(line=e.lineno, character=e.offset+1) if sys.version_info < (3, 10) else \
                        Position(line=e.end_lineno, character=e.end_offset)
                ),
                message = type(e).__name__ + ": " + e.msg,
                source = EXTENSION_NAME
            )])
            return False
        except ValueError as e:
            print("Unable to parse file: " + self.filePath + ". Value error.")
            return False
        return True

    def visit_Import(self, node):
        self._resolve_import(None, [name.name for name in node.names], 0)

    def visit_ImportFrom(self, node):
        self._resolve_import(node.module, [name.name for name in node.names], node.level)

    def _resolve_import(self, from_stmt, names, level):
        packages = []
        if level != 0:
            if level > len(Path(self.filePath).parts):
                print("ERROR: level is too big ! The current path doesn't have enough parents")
                return
            packages = self.symbol.get_ancestors() + [self.symbol.name] if level == 1 else self.symbol.get_ancestors()[:-level+1]

        for name in names:
            elements = from_stmt.split(".") if from_stmt != None else []
            elements.append(name)
            
            for element in elements:
                current_symbol = OdooBase.get().symbols.get_symbol(packages)
                next_step_symbols = OdooBase.get().symbols.get_symbol(packages + [element])
                if not next_step_symbols:
                    symbol_paths = current_symbol.paths
                    for path in symbol_paths:
                        full_path = path + os.sep + element
                        if os.path.isdir(full_path):
                            parser = PythonParser(self.ls, full_path, current_symbol)
                            parser.load_symbols()
                            break
                        elif os.path.isfile(full_path + ".py"):
                            parser = PythonParser(self.ls, full_path + ".py", current_symbol)
                            parser.load_symbols()
                            break
                else:
                    packages += [element]


    def visit_ClassDef(self, node):
        for base in node.bases:
            pass
        if node.name not in self.symbol.symbols:
            self.symbol.add_symbol([], Symbol(node.name, "class", self.filePath))

    def load_symbols(self):
        """ Load all symbols from the file or package """
        if self.symbol.get_symbol(self.filePath.split(os.sep)[-1].split(".py")[0]):
            return
        if not self.filePath.endswith(".py"):
            #check if this is a package:
            if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                symbol = Symbol(self.filePath.split(os.sep)[-1], "package", self.filePath)
                self.symbol.add_symbol([], symbol)
                self.symbol = symbol
                self.filePath = os.path.join(self.filePath, "__init__.py")
            else:
                return
        else:
            symbol = Symbol(self.filePath.split(os.sep)[-1].split(".py")[0], "file", self.filePath)
            self.symbol.add_symbol([], symbol)
            self.symbol = symbol
        self.mode = 'symbols'
        self.parse()