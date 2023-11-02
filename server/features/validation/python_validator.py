import ast
import os
import sys
from ...constants import *
from ...core.odoo import Odoo
from ...core.symbol import Symbol
from ...python_utils import PythonUtils
from ...core.file_mgr import FileMgr
from ...core.import_resolver import resolve_import_stmt
from lsprotocol.types import (Diagnostic,Position, Range)

class ClassContentCacheValidator():

    def __init__(self):
        self.modelName = None
        self.modelInherit = []
        self.modelInherits = []
        self.log_access = True

class PythonValidator(ast.NodeVisitor):
    """The python Validator aims to validate the end code in symbols. No structural changes are allowed here. No new
    symbol, no symbol deletion. however, each line of code is validated and diagnostics are thrown to client if the code
    can't be validated"""

    def __init__(self, ls, symbol):
        """Prepare a validator to validate the given file. """
        self.symStack = [symbol.get_in_parents([SymType.FILE]) or symbol] # we always validate at file level
        self.classContentCache = []
        self.ls = ls
        self.currentModule = None
        self.filePath = None
        self.safeImport = [False] # if True, we are in a safe import (surrounded by try except)
        self.pathTree = [] #cache tree of the current symbol

    def validate(self):
        """validate the symbol"""
        self.diagnostics = []
        if self.symStack[0].validationStatus:
            return
        if self.symStack[0].type in [SymType.NAMESPACE]:
            return
        elif self.symStack[0].type == SymType.PACKAGE:
            self.filePath = os.path.join(self.symStack[0].paths[0], "__init__.py" + self.symStack[0].i_ext)
        else:
            self.filePath = self.symStack[0].paths[0]
        self.symStack[0].validationStatus = 1
        if DEBUG_VALIDATION:
            print("Load validation: " + self.filePath)
        self.tree = self.symStack[-1].get_tree()
        fileInfo = FileMgr.get_file_info(self.filePath)
        if not fileInfo.ast: #doesn"t compile or we don't want to validate it
            return
        self.validate_ast(fileInfo.ast)
        self.validate_structure()
        self.symStack[0].validationStatus = 2
        fileInfo.replace_diagnostics(BuildSteps.VALIDATION, self.diagnostics)
        #publish diag in all case to erase potential previous diag
        if self.symStack[0].in_workspace:
            fileInfo.publish_diagnostics(self.ls)
        else:
            FileMgr.delete_info(self.filePath)
            self.symStack[0].ast_node = None

    def validate_ast(self, ast):
        self.currentModule = self.symStack[-1].get_module_sym()
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
        symbols = resolve_import_stmt(self.ls, self.symStack[0], self.symStack[-1], from_stmt, name_aliases, level, (node.lineno, node.col_offset), (node.end_lineno, node.end_col_offset))

        for node_alias, symbol, file_tree in symbols:
            name = node_alias.name
            if symbol:
                module = symbol.get_module_sym()
                if module and not self.currentModule.is_in_deps(module.dir_name) and not self.safeImport[-1]:
                    self.diagnostics.append(Diagnostic(
                        range = Range(
                            start=Position(line=node.lineno-1, character=node.col_offset),
                            end=Position(line=node.lineno-1, character=1) if sys.version_info < (3, 8) else \
                                Position(line=node.lineno-1, character=node.end_col_offset)
                        ),
                        message = module.dir_name + " is not in the dependencies of the module",
                        source = EXTENSION_NAME,
                        severity = 2
                    ))

    def visit_Assign(self, node):
        return
        assigns = self.unpack_assign(node.targets, node.value, {})
        for variable, value in assigns.items():
            #TODO add other inference type than Name
            if isinstance(value, ast.Name):
                infered = Inferencer.inferNameInScope(value.id, value.lineno, self.symStack[-1])
                if infered:
                    symbol_infer = infered.symbol
                self.symStack[-1].inferencer.addInference(Inference(
                    variable,
                    symbol_infer if infered else None,
                    node.lineno
                ))
            if isinstance(value, ast.Call):
                symbol_infer = PythonUtils.evaluateTypeAST(value, self.symStack[-1])
                if symbol_infer:
                    symbol_infer = symbol_infer.eval
                self.symStack[-1].inferencer.addInference(Inference(
                    variable,
                    symbol_infer,
                    node.lineno
                ))
            if self.symStack[-1].type == "class":
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
            if self.symStack[-1].type in ["class", "file"]:
                if variable not in self.symStack[-1].symbols:
                    variable = Symbol(variable, "variable")
                    variable.startLine = node.lineno
                    variable.endLine = node.end_lineno
                    variable.eval = PythonUtils.evaluateTypeAST(value, self.symStack[-1])
                    self.symStack[-1].add_symbol(variable)
                else:
                    print("Warning: symbol already defined")

    def visit_FunctionDef(self, node):
        return
        symbol = Symbol(node.name, "function")
        symbol.startLine = node.lineno
        symbol.endLine = node.end_lineno
        if self.symStack[-1].type in ["file", "function"]:
            self.symStack[-1].inferencer.addInference(Inference(
                node.name,
                symbol,
                node.lineno
            ))
        self.symStack[-1].add_symbol(symbol)
        self.symStack.append(symbol)
        ast.NodeVisitor.generic_visit(self, node)
        self.symStack.pop()

    def _extract_base_name(attr):
        return
        if isinstance(attr, ast.Name):
            return attr.id
        elif isinstance(attr, ast.Attribute):
            return PythonArchBuilder._extract_base_name(attr.value) + "." + attr.attr
        elif isinstance(attr, ast.Call):
            pass
        return ""

    def visit_ClassDef(self, node):
        return
        bases = []
        #search for base classes symbols
        for base in node.bases:
            full_base = PythonArchBuilder._extract_base_name(base)
            if full_base:
                inference = self.symStack[-1].infer_name(full_base.split(".")[0], node.lineno)
                if not inference or not inference.symbol:
                    continue
                symbol = inference.symbol
                if len(full_base.split(".")) > 1:
                    symbol = symbol.get_symbol(full_base.split(".")[1:])
                if symbol:
                    if symbol.type == "class":
                        bases += [symbol]
                    elif symbol.type == "variable":
                        #Ouch, this is not a class :/ Last chance, we can try to evaluate the variable to check if an inferencer has linked it to a Class
                        inferred = symbol.parent.inferencer.infer_name(symbol.name, 10000000) #TODO 1000000 ?wtf?
                        if inferred and inferred.symbol and inferred.symbol.type == "class":
                            bases += [inferred.symbol]
        if node.name not in self.symStack[-1].symbols:
            symbol = Symbol(node.name, "class", self.filePath)
            symbol.evaluationType = symbol
            symbol.startLine = node.lineno
            symbol.endLine = node.end_lineno
            symbol.classData = ClassData()
            self.symStack[-1].inferencer.addInference(Inference(
                node.name,
                symbol,
                node.lineno
            ))
            self.symStack[-1].add_symbol(symbol)
            self.symStack.append(symbol)
            for base in bases:
                if symbol.get_tree() not in base.dependents:
                    base.dependents.append(symbol.get_tree())
                symbol.classData.bases.append(base.get_tree())
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
                    orig_module = Odoo.get().models[inh].get_main_symbol().get_module_sym()
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
            self.symStack.pop()

    def add_magic_fields(self, symbol, node, data):
        def create_symbol(name, type, lineno):
            variable = Symbol(name, SymType.VARIABLE)
            variable.start_pos = (lineno, 0)
            variable.end_pos =(lineno, 1)
            #TODO adapt
            #variable.eval = Symbol(type, SymType.PRIMITIVE)
            symbol.add_symbol(variable)
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

    def validate_structure(self):
        for symbol in self.symStack[0].get_ordered_symbols():
            if symbol.type == SymType.CLASS:
                if symbol.modelData:
                    position = (symbol.start_pos, symbol.end_pos)
                    _inherit_decl = symbol.get_symbol([], ["_inherit"])
                    if _inherit_decl:
                        position = (_inherit_decl.start_pos, _inherit_decl.end_pos)
                    for inherit in symbol.modelData.inherit:
                        if inherit == "base":
                            continue
                        model = Odoo.get().models.get(inherit)
                        if not model:
                            self.diagnostics.append(Diagnostic(
                                range = Range(
                                    start=Position(line=position[0][0]-1, character=position[0][1]),
                                    end=Position(line=position[0][0]-1, character=1) if sys.version_info < (3, 8) else \
                                        Position(line=position[0][0]-1, character=position[1][0])
                                ),
                                message = inherit + " does not exist",
                                source = EXTENSION_NAME,
                                severity = 1
                            ))
                        else:
                            inherited_models = model.get_main_symbols(self.currentModule)
                            if len(inherited_models) > 1:
                                self.diagnostics.append(Diagnostic(
                                range = Range(
                                    start=Position(line=position[0][0]-1, character=position[0][1]),
                                    end=Position(line=position[0][0]-1, character=1) if sys.version_info < (3, 8) else \
                                        Position(line=position[0][0]-1, character=position[1][0])
                                ),
                                message = "This model is ambiguous. Please fix your dependencies or avoid using same model name in different modules",
                                source = EXTENSION_NAME,
                                severity = 1
                            ))
                            elif not inherited_models:
                                model_syms = model.get_main_symbols()
                                models = []
                                for model_sym in model_syms:
                                    models.append(model_sym.get_module_sym())
                                models = ", ".join([model.name for model in models])
                                models = " or ".join(models.rsplit(", ", 1))
                                self.diagnostics.append(Diagnostic(
                                    range = Range(
                                        start=Position(line=position[0][0]-1, character=position[0][1]),
                                        end=Position(line=position[0][0]-1, character=1) if sys.version_info < (3, 8) else \
                                            Position(line=position[0][0]-1, character=position[1][0])
                                    ),
                                    message = inherit + " is not in your dependencies. Please review the dependencies of your module to add " + models + " in it",
                                    source = EXTENSION_NAME,
                                    severity = 1
                                ))