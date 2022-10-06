import ast
import os
import sys
from pathlib import Path
from server.odooBase import Symbol, OdooBase, Model

class PythonParser(ast.NodeVisitor):
    """This class read a file and extract all relevant data. Classes, functions and models are stored in odooBase.
    It imports also all needed imports and store relevant data from them into odooBase."""

    parsed = {}

    def __init__(self, ls, path, symbol):
        self.filePath = path
        self.symbol = symbol # symbol we are parsing
        self.mode = 'symbols'
        self.ls = ls
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
        self._resolve_import(None, [(name.name, name.asname) for name in node.names], 0)

    def visit_ImportFrom(self, node):
        self._resolve_import(node.module, [(name.name, name.asname) for name in node.names], node.level)

    def _resolve_import(self, from_stmt, names, level):
        packages = []
        if level != 0:
            if level > len(Path(self.filePath).parts):
                print("ERROR: level is too big ! The current path doesn't have enough parents")
                return
            if self.symbol.type == "package":
                #as we are at directory level, not init.py
                level -= 1
            if level == 0:
                packages = self.symbol.get_tree()
            else:
                packages = self.symbol.get_tree()[:-level]

        for name, asname in names:
            packages_copy = packages[:]
            elements = from_stmt.split(".") if from_stmt != None else []
            elements += name.split(".") if name != None else []
            importSymbol = None

            for element in elements:
                if element != '*':
                    current_symbol = OdooBase.get().symbols.get_symbol(packages_copy)
                    if not current_symbol:
                        if packages_copy and packages_copy[0] == "odoo":
                            print(packages_copy)
                        break
                    next_step_symbols = OdooBase.get().symbols.get_symbol(packages_copy + [element])
                    if not next_step_symbols:
                        symbol_paths = current_symbol.paths if current_symbol else []
                        for path in symbol_paths:
                            full_path = path + os.sep + element
                            if os.path.isdir(full_path):
                                parser = PythonParser(self.ls, full_path, current_symbol)
                                importSymbol = parser.load_symbols()
                                break
                            elif os.path.isfile(full_path + ".py"):
                                parser = PythonParser(self.ls, full_path + ".py", current_symbol)
                                importSymbol = parser.load_symbols()
                                break
                    else:
                        importSymbol = next_step_symbols
                    packages_copy += [element]
                else:
                    # in case of *, we have to populate localAliases with relevant symbols and import all subsymbols in current symbol
                    # this implementation respects the python import order, and submodules will be imported too only if they are known 
                    # due to a previous import statement.
                    to_browse = [importSymbol]
                    while to_browse:
                        current_symbol = to_browse.pop()
                        for symbol in current_symbol.symbols.values():
                            if symbol.type == "package":
                                to_browse.append(symbol)
                            elif symbol.type in ["class", "function", "variable"]:
                                self.symbol.localAliases[symbol.name] = symbol
                                self.symbol.add_symbol([], symbol)
                if elements[-1] != '*':
                    self.symbol.localAliases[asname if asname else name] = OdooBase.get().symbols.get_symbol(packages_copy)

    def _extract_base_name(attr):
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonParser._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
    
    def visit_Assign(self, node):
        assigns = self.unpack_assign(node.targets, node.value, self.symbol, {})
        for variable, value in assigns.items():
            if isinstance(value, ast.Name) and value.id in self.symbol.localAliases:
                self.symbol.localAliases[variable] = self.symbol.localAliases[value.id]

    def visit_ClassDef(self, node):
        bases = []
        for base in node.bases:
            full_base = PythonParser._extract_base_name(base)
            if full_base and full_base.split(".")[0] in self.symbol.localAliases:
                symbol = self.symbol.localAliases[full_base.split(".")[0]]
                if not symbol:
                    continue
                if len(full_base.split(".")) > 1:
                    symbol = symbol.get_symbol(full_base.split(".")[1:])
                if symbol and symbol.type == "class":
                    bases += [symbol]
        if node.name not in self.symbol.symbols:
            symbol = Symbol(node.name, "class", self.filePath)
            symbol.bases = bases
            self.symbol.localAliases[node.name] = symbol
            self.symbol.add_symbol([], symbol)
            #parse body
            self._visit_Body(node.body, symbol)
    
    def unpack_assign(self, node_targets, node_values, symbol, acc = {}):
        """ Unpack assignement to extract variables and values.
            This method will return a dictionnary that hold each variables and the set value (still in ast node)
            example: variable, variable2 = "test"
            ast.Assign => {"variable": ast.Node("test"), "variable2": ast.Node("test")}
            value is of the right type (ast.Constant, ast.Tuple...)
         """
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
                        self.unpack_assign(nt, nv, symbol, acc)
            elif isinstance(target, ast.Tuple):
                for elt in target.elts:
                    acc[elt.id] = node_values
            else:
                pass
                # print("ERROR: unpack_assign not implemented for " + str(node_targets) + " and " + str(node_values))
        return acc

    def _visit_Body(self, body, symbol):
        modelName = None
        modelInherit = None
        modelInherits = None
        fields = None
        funcs = None
        #load inheritance
        for base in symbol.bases:
            pass #TODO
        #load body
        for node in body:
            if isinstance(node, ast.FunctionDef):
                self._visit_FunctionDef(node, symbol)
            elif isinstance(node, ast.Assign):
                assigns = self.unpack_assign(node.targets, node.value, symbol, {})
                for name, value in assigns.items():
                    if name == "_name":
                        #class name
                        if isinstance(value, ast.Constant) and value.value != None: #can be None for baseModel
                            modelName = value.value
                    elif name == "_inherit":
                        #class name
                        if isinstance(value, ast.Constant):
                            modelInherit = [value.value]
                        elif isinstance(value, ast.List):
                            modelInherit = []
                            for v in value.elts:
                                if isinstance(v, ast.Constant):
                                    modelInherit += [v.value]
                    elif name == "_inherits":
                        #class name
                        if isinstance(value, ast.Dict):
                            modelInherits = {}
                            for k, v in zip(value.keys, value.values):
                                if isinstance(k, ast.Constant) and isinstance(v, ast.Constant):
                                    modelInherits[k.value] = v.value

            elif isinstance(node, ast.AnnAssign):
                pass #TODO not handled yet
            elif isinstance(node, ast.Import):
                self.visit_Import(node)
            elif isinstance(node, ast.ImportFrom):
                self.visit_ImportFrom(node)
        # parsing is done, now we can save what is found. The saving is deferred to the parsing to fix wrong orders in declarations
        # (inherit before name for example)
        if modelName:
            symbol.modelName = modelName
            if modelName not in OdooBase.get().models:
                OdooBase.get().models[modelName] = Model(modelName)
            OdooBase.get().models[modelName].symbols.append(symbol)

    def _visit_FunctionDef(self, node, classSymbol):
        if node.name not in classSymbol.localAliases:
            symbol = Symbol(node.name, "function", self.filePath)
            classSymbol.localAliases[node.name] = symbol
            classSymbol.add_symbol([], symbol)

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
                symbol = Symbol(self.filePath.split(os.sep)[-1], "namespace", self.filePath)
                self.symbol.add_symbol([], symbol)
                self.symbol = symbol
                return self.symbol
        else:
            symbol = Symbol(self.filePath.split(os.sep)[-1].split(".py")[0], "file", self.filePath)
            self.symbol.add_symbol([], symbol)
            self.symbol = symbol
        self.mode = 'symbols'
        self.parse()
        return self.symbol