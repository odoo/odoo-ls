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

class PythonParser(ast.NodeVisitor):
    """This class is linked to a Symbol and can extract/build relatad data. 
    It can be used to load depending Symbols from python files."""

    parsed = {}

    def __init__(self, ls, path, symbol):
        self.grammar = Odoo.get().grammar
        self.filePath = path
        self.symbol = [symbol] # symbols we are parsing
        self.classContentCache = []
        self.safeImport = [False] # if True, we are in a safe import (surrounded by try except)
        self.ls = ls
        self.diagnostics = []
        self.currentModule = None
    
    def load_symbols(self):
        """ Load all symbols from the symbol. It will follow all imports and create new Symbols from files """
        if self.symbol[0].get_symbol(self.filePath.split(os.sep)[-1].split(".py")[0]):
            return
        self.diagnostics = []
        #If this is not a python file, check that this is a package and load it
        if not self.filePath.endswith(".py"):
            #check if this is a package:
            if os.path.exists(os.path.join(self.filePath, "__init__.py")):
                symbol = Symbol(self.filePath.split(os.sep)[-1], "package", self.filePath)
                if self.symbol[0].get_tree() == ["odoo", "addons"] and \
                    os.path.exists(os.path.join(self.filePath, "__manifest__.py")):
                    symbol.isModule = True
                self.symbol[0].add_symbol([], symbol)
                self.symbol[0] = symbol
                self.filePath = os.path.join(self.filePath, "__init__.py")
            else:
                symbol = Symbol(self.filePath.split(os.sep)[-1], "namespace", self.filePath)
                self.symbol[0].add_symbol([], symbol)
                self.symbol[0] = symbol
                return self.symbol[0]
        else:
            symbol = Symbol(self.filePath.split(os.sep)[-1].split(".py")[0], "file", self.filePath)
            self.symbol[0].add_symbol([], symbol)
            self.symbol[0] = symbol
        #parse the Python file
        Odoo.get().files[self.filePath] = self.symbol[0]
        moduleName = self.symbol[0].getModule()
        if moduleName and moduleName != 'base' or moduleName in Odoo.get().modules: #TODO hack to be able to import from base when no module has been loaded yet (example services/server.py line 429 in master)
            self.currentModule = Odoo.get().modules[moduleName]
        try:
            with open(self.filePath, "rb") as f:
                content = f.read()
            #tree = self.grammar.parse(content, error_recovery=False, path=self.filePath, cache = False)
            tree = ast.parse(content, self.filePath)
            self.visit(tree)
        except SyntaxError as e:
            self.ls.publish_diagnostics(pathname2uri(self.filePath), [Diagnostic(
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
        except Exception as e:
            import traceback
            a = traceback.format_exc()
            print(e)#TODO remove
        if self.diagnostics:
            self.ls.publish_diagnostics(pathname2uri(self.filePath), self.diagnostics)
        return self.symbol[0]

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
            if self.symbol[0].type == "package":
                #as we are at directory level, not init.py
                level -= 1
            if level == 0:
                packages = self.symbol[0].get_tree()
            else:
                packages = self.symbol[0].get_tree()[:-level]

        for name, asname in names:
            packages_copy = packages[:]
            elements = from_stmt.split(".") if from_stmt != None else []
            elements += name.split(".") if name != None else []
            importSymbol = None

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
                                    module = self.symbol[-1].getModule()
                                    if module and not Odoo.get().modules[module].is_in_deps(element) and not self.safeImport[-1]:
                                        print(element + " has not been loaded. It should be in dependencies of " + module)
                                        self.diagnostics.append(Diagnostic(
                                            range = Range(
                                                start=Position(line=node.lineno-1, character=node.col_offset),
                                                end=Position(line=node.lineno-1, character=1) if sys.version_info < (3, 8) else \
                                                    Position(line=node.lineno-1, character=node.end_col_offset)
                                            ),
                                            message = element + " has not been loaded. It should be in dependencies of " + module,
                                            source = EXTENSION_NAME
                                        ))
                                        return
                                    if not module:
                                        """If we are searching for a odoo.addons.* element, skip it if we are not in a module.
                                        It means we are in a file like odoo/*, and modules are not loaded yet."""
                                        return
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
                    # in case of *, we have to populate inferencer with relevant symbols and import all subsymbols in current symbol
                    # this implementation respects the python import order, and submodules will be imported too only if they are known 
                    # due to a previous import statement.
                    to_browse = [importSymbol]
                    while to_browse:
                        current_symbol = to_browse.pop()
                        for symbol in current_symbol.symbols.values():
                            if symbol.type == "package":
                                to_browse.append(symbol)
                            elif symbol.type in ["class", "function", "variable"]:
                                self.symbol[-1].inferencer.addInference(Inference(symbol.name, symbol, node.lineno))
            if elements[-1] != '*':
                self.symbol[-1].inferencer.addInference(
                    Inference(asname if asname else name, Odoo.get().symbols.get_symbol(packages_copy), node.lineno)
                )
    
    def visit_Assign(self, node):
        assigns = self.unpack_assign(node.targets, node.value, {})
        for variable, value in assigns.items():
            #TODO add other inference type than Name
            if isinstance(value, ast.Name):
                infered = Inferencer.inferNameInScope(value.id, value.lineno, self.symbol[-1])
                if infered:
                    symbol_infer = infered.symbol
                self.symbol[-1].inferencer.addInference(Inference(
                    variable,
                    symbol_infer if infered else None,
                    node.lineno
                ))
            if isinstance(value, ast.Call):
                symbol_infer = PythonUtils.evaluateTypeAST(value, self.symbol[-1])
                if symbol_infer:
                    symbol_infer = symbol_infer.evaluationType
                self.symbol[-1].inferencer.addInference(Inference(
                    variable,
                    symbol_infer,
                    node.lineno
                ))
            if self.symbol[-1].type == "class":
                if variable == "_name":
                    if isinstance(value, ast.Constant) and value.value != None: #can be None for baseModel
                        self.classContentCache[-1].modelName = value.value
                elif variable == "_inherit":
                    if isinstance(value, ast.Constant):
                        self.classContentCache[-1].modelInherit = [value.value]
                    elif isinstance(value, ast.List):
                        self.classContentCache[-1].modelInherit = []
                        for v in value.elts:
                            if isinstance(v, ast.Constant):
                                self.classContentCache[-1].modelInherit += [v.value]
                elif variable == "_inherits":
                    if isinstance(value, ast.Dict):
                        self.classContentCache[-1].modelInherits = {}
                        for k, v in zip(value.keys, value.values):
                            if isinstance(k, ast.Constant) and isinstance(v, ast.Constant):
                                self.classContentCache[-1].modelInherits[k.value] = v.value
                elif variable == "_log_access":
                    if isinstance(value, ast.Constant):
                        self.classContentCache[-1].log_access = bool(value.value)
            if self.symbol[-1].type in ["class", "file"]:
                if variable not in self.symbol[-1].symbols:
                    variable = Symbol(variable, "variable", self.filePath)
                    variable.startLine = node.lineno
                    variable.endLine = node.end_lineno
                    variable.evaluationType = PythonUtils.evaluateTypeAST(value, self.symbol[-1])
                    self.symbol[-1].add_symbol([], variable)
                else:
                    print("Warning: symbol already defined")

    def visit_FunctionDef(self, node):
        symbol = Symbol(node.name, "function", self.filePath)
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        if self.symbol[-1].type in ["file", "function"]:
            self.symbol[-1].inferencer.addInference(Inference(
                node.name,
                symbol,
                node.lineno
            ))
        self.symbol[-1].add_symbol([], symbol)
        self.symbol.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symbol.pop()
    
    def _extract_base_name(attr):
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonParser._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
        return ""

    def visit_ClassDef(self, node):
        bases = []
        #search for base classes symbols
        for base in node.bases:
            full_base = PythonParser._extract_base_name(base)
            if full_base:
                inference = self.symbol[-1].inferName(full_base.split(".")[0], node.lineno)
                if not inference or not inference.symbol:
                    continue
                symbol = inference.symbol
                if len(full_base.split(".")) > 1:
                    symbol = symbol.get_symbol(full_base.split(".")[1:])
                if symbol:
                    if symbol.type == "class":
                        bases += [symbol] #TODO DON'T BASE ON REF ?
                    elif symbol.type == "variable":
                        #Ouch, this is not a class :/ Last chance, we can try to evaluate the variable to check if an inferencer has linked it to a Class
                        inferred = symbol.parent.inferencer.inferName(symbol.name, 10000000)
                        if inferred and inferred.symbol and inferred.symbol.type == "class":
                            bases += [inferred.symbol]
        if node.name not in self.symbol[-1].symbols:
            symbol = Symbol(node.name, "class", self.filePath)
            symbol.evaluationType = symbol
            symbol.startLine = node.lineno
            symbol.endLine = node.end_lineno
            symbol.classData = ClassData()
            symbol.classData.bases = bases
            self.symbol[-1].inferencer.addInference(Inference(
                node.name,
                symbol,
                node.lineno
            ))
            self.symbol[-1].add_symbol([], symbol)
            self.symbol.append(symbol)
            self.classContentCache.append(ClassContentCache())
            ast.NodeVisitor.generic_visit(self, node)
            data = self.classContentCache.pop()
            if symbol.is_inheriting_from(["odoo", "models", "BaseModel"]):
                symbol.classData.modelData = ModelData()
            if data.modelInherit and not data.modelName:
                data.modelName = data.modelInherit[0] if len(data.modelInherit) == 1 else symbol.name #v15 behaviour
            for inh in data.modelInherit:
                orig_module = ""
                if inh in Odoo.get().models:
                    orig_module = Odoo.get().models[inh].get_main_symbol().getModule()
                if not orig_module or (not self.currentModule.is_in_deps(orig_module) and \
                    orig_module != self.currentModule.dir_name):
                    self.diagnostics.append(Diagnostic(
                        range = Range(
                            start=Position(line=node.lineno-1, character=node.col_offset),
                            end=Position(line=node.lineno-1, character=500)
                        ),
                        message = node.name + " is inheriting from " + inh + " but this model is not defined in any loaded module. Please fix the dependencies of the module " + self.currentModule.name,
                        source = EXTENSION_NAME
                    ))
                else:
                    symbol.classData.modelData.inherit.append(inh)
            if data.modelName:
                if not symbol.classData.modelData:
                    print("oups")
                symbol.classData.modelData.name = data.modelName
                if data.modelName not in Odoo.get().models:
                    Odoo.get().models[data.modelName] = Model(data.modelName, symbol)
                else:
                    Odoo.get().models[data.modelName].impl_sym.append(symbol)
            self.add_magic_fields(symbol, node, data)
            self.symbol.pop()
    
    def add_magic_fields(self, symbol, node, data):
        def create_symbol(name, type, lineno):
            variable = Symbol(name, "variable", self.filePath)
            variable.startLine = lineno
            variable.endLine = lineno
            variable.evaluationType = Symbol(type, "primitive", "")
            symbol.add_symbol([], variable)
            return variable
        if symbol.get_tree() == ["odoo", "models", "Model"]:
            create_symbol("id", "constant", node.lineno)
            create_symbol("display_name", "constant", node.lineno)
            create_symbol("_log_access", "constant", node.lineno)
            if data.log_access:
                create_symbol("create_date", "constant", node.lineno)
                create_symbol("create_uid", "constant", node.lineno)
                create_symbol("write_date", "constant", node.lineno)
                create_symbol("write_uid", "constant", node.lineno)


    def unpack_assign(self, node_targets, node_values, acc = {}):
        """ Unpack assignement to extract variables and values.
            This method will return a dictionnary that hold each variables and the set value (still in ast node)
            example: variable = variable2 = "test" (2 targets, 1 value)
            ast.Assign => {"variable": ast.Node("test"), "variable2": ast.Node("test")}
         """
        try:
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
            
        except Exception as e:
            print("here")
        return acc
