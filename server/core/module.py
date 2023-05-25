import ast
from server.constants import *
import os
from .odoo import * 
from server.core.fileMgr import *
import weakref
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

class Module():

    rootPath = ""
    loaded = False

    name = ""
    dir_name = ""
    depends = ["base"]
    data = []

    def __init__(self, ls, dir_path):
        self.dir_name = os.path.split(dir_path)[1]
        self.rootPath = dir_path
        manifestPath = os.path.join(dir_path, "__manifest__.py")
        if not os.path.exists(manifestPath):
            return
        diagnostics = []
        diagnostics += self.load_manifest(os.path.join(dir_path, "__manifest__.py"))
        if self.dir_name in Odoo.get().modules:
            #TODO merge ! or erase? or raise error? :(
            print("already in: " + self.dir_name)
        Odoo.get().modules[self.dir_name] = self
        if diagnostics:
            ls.publish_diagnostics(FileMgr.pathname2uri(manifestPath), diagnostics)

    def load_arch(self, ls):
        if self.loaded:
            return
        diagnostics = []
        diagnostics += self.load_depends(ls)
        diagnostics += self._load_data()
        diagnostics += self._load_arch(ls, self.rootPath)
        self.loaded = True
        print("loaded: " + self.dir_name)
        if diagnostics:
            ls.publish_diagnostics(FileMgr.pathname2uri(os.path.join(self.rootPath, "__manifest__.py")), diagnostics)

    def load_manifest(self, manifestPath):
        """ Load manifest to identify the module characteristics 
        Returns list of diagnostics to publish in manifest file """
        with open(manifestPath, "r", encoding="utf8") as f:
            md = f.read()
            dic = ast.literal_eval(md)
            self.name = dic.get("name", "")
            self.depends = dic.get("depends", [])
            if self.dir_name != 'base':
                self.depends.append("base")
            self.data = dic.get("data", [])
        return []

    def load_depends(self, ls):
        """ ensure that all modules indicates in the module dependencies are well loaded.
        Returns list of diagnostics to publish in manifest file """
        diagnostics = []
        for depend in self.depends:
            if depend in Odoo.get().modules:
                Odoo.get().modules[depend].load_arch(ls)
            else:
                diagnostics.append(Diagnostic(
                    range = Range(
                        start=Position(line=0, character=0),
                        end=Position(line=0, character=1)
                    ),
                    message = f"Module {self.name} depends on {depend} which is not found. Please check your addonsPaths.",
                    source = EXTENSION_NAME
                ))
        return diagnostics
    
    def _load_data(self):
        return []

    def _load_arch(self, ls, path):
        parser = PythonArchBuilder(ls, Odoo.get().symbols.get_symbol(["odoo", "addons"]), path)
        parser.load_arch()
        return []
    
    def is_in_deps(self, module_name):
        if self.dir_name == module_name or module_name in self.depends:
            return True
        for dep in self.depends:
            is_in = Odoo.get().modules[dep].is_in_deps(module_name)
            if is_in:
                return True
        return False
