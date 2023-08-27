import ast
from server.constants import *
import os
from .odoo import *
from server.core.fileMgr import *
from server.core.pythonArchBuilder import *
from server.features.validation.pythonValidator import *
from lsprotocol.types import (CompletionItem, CompletionList, CompletionOptions,
                             CompletionParams, ConfigurationItem,
                             ConfigurationParams, Diagnostic,
                             DidChangeTextDocumentParams,
                             DidCloseTextDocumentParams,
                             DidOpenTextDocumentParams, MessageType, Position,
                             Range, Registration, RegistrationParams,
                             SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
                             Unregistration, UnregistrationParams)

class ModuleSymbol(Symbol):

    rootPath = ""
    loaded = False

    module_name = ""
    dir_name = ""
    depends = ["base"]
    data = []

    def __init__(self, ls, dir_path):
        self.valid = True
        self.dir_name = os.path.split(dir_path)[1]
        print("loading " + self.dir_name)
        super().__init__(self.dir_name, SymType.PACKAGE, dir_path)
        self.rootPath = dir_path
        manifestPath = os.path.join(dir_path, "__manifest__.py")
        if not os.path.exists(manifestPath):
            self.valid = False
            return
        diagnostics = []
        diagnostics += self.load_manifest(os.path.join(dir_path, "__manifest__.py"))
        if self.dir_name in Odoo.get().modules:
            pass
        Odoo.get().modules[self.dir_name] = RegisteredRef(self)
        f = FileMgr.getFileInfo(manifestPath)
        f.replace_diagnostics(BuildSteps.ARCH, diagnostics)
        f.publish_diagnostics(ls)

    def load_module_info(self, ls):
        if self.loaded:
            return []
        loaded = []
        diagnostics, loaded_modules = self.load_depends(ls)
        loaded += loaded_modules
        diagnostics += self._load_data()
        diagnostics += self._load_arch(ls, self.rootPath)
        self.loaded = True
        loaded.append(self.dir_name)
        f = FileMgr.getFileInfo(os.path.join(self.rootPath, "__manifest__.py"))
        f.replace_diagnostics(BuildSteps.ARCH, diagnostics)
        f.publish_diagnostics(ls)
        return loaded

    def load_manifest(self, manifestPath):
        """ Load manifest to identify the module characteristics
        Returns list of diagnostics to publish in manifest file """
        with open(manifestPath, "r", encoding="utf8") as f:
            md = f.read()
            dic = ast.literal_eval(md)
            self.module_name = dic.get("name", "")
            self.depends = dic.get("depends", [])
            if self.dir_name != 'base':
                self.depends.append("base")
            self.data = dic.get("data", [])
        return []

    def load_depends(self, ls):
        """ ensure that all modules indicates in the module dependencies are well loaded.
        Returns list of diagnostics to publish in manifest file """
        diagnostics = []
        loaded = []
        for depend in self.depends:
            if depend not in Odoo.get().modules:
                from server.core.importResolver import resolve_import_stmt
                odoo_addons = Odoo.get().get_symbol(["odoo", "addons"], [])
                alias = [ast.alias(name=depend, asname=None)]
                _, dep_module, _ = resolve_import_stmt(ls, odoo_addons, odoo_addons, None, alias, 1, 0, 0)
                if not dep_module:
                    diagnostics.append(Diagnostic(
                        range = Range(
                            start=Position(line=0, character=0),
                            end=Position(line=0, character=1)
                        ),
                        message = f"Module {self.name} depends on {depend} which is not found. Please check your addonsPaths.",
                        source = EXTENSION_NAME
                    ))
                else:
                    self.add_dependency(dep_module, BuildSteps.ARCH, BuildSteps.ARCH)
        return diagnostics, loaded

    def _load_data(self):
        return []

    def _load_arch(self, ls, path):
        if os.path.exists(os.path.join(path, "tests")):
            tests_parser = PythonArchBuilder(ls, self, os.path.join(path, "tests"))
            tests_parser.load_arch()
        return []

    def is_in_deps(self, dir_name):
        if self.dir_name == dir_name or dir_name in self.depends:
            return True
        for dep in self.depends:
            dep_module = Odoo.get().modules.get(dep, None)
            if not dep_module:
                continue
            is_in = dep_module.ref.is_in_deps(dir_name)
            if is_in:
                return True
        return False
