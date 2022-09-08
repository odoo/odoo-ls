import ast
from .constants import *
import os
import server.odooBase as odooBase
from server.pythonParser import PythonParser
from pygls.lsp.types import (CompletionItem, CompletionList, CompletionOptions,
                             CompletionParams, ConfigurationItem,
                             ConfigurationParams, Diagnostic,
                             DidChangeTextDocumentParams,
                             DidCloseTextDocumentParams,
                             DidOpenTextDocumentParams, MessageType, Position,
                             Range, Registration, RegistrationParams,
                             SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
                             Unregistration, UnregistrationParams)

class OdooModule():

    #static
    modules = {}

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
        OdooModule.modules[self.dir_name] = self
        ls.publish_diagnostics(manifestPath, diagnostics)

    def load(self, ls):
        if self.loaded:
            return
        diagnostics = []
        diagnostics += self.load_depends(ls)
        diagnostics += self.load_data()
        diagnostics += self.load_python_files(ls, self.rootPath)
        self.loaded = True
        ls.publish_diagnostics(os.path.join(self.rootPath, "__manifest__.py"), diagnostics)
    
    def load_manifest(self, manifestPath):
        """ Load manifest to identify the module characteristics 
        Returns list of diagnostics to publish in manifest file """
        with open(manifestPath, "r") as f:
            md = f.read()
            dic = ast.literal_eval(md)
            self.name = dic.get("name", "")
            self.depends = dic.get("depends", [])
            self.data = dic.get("data", [])
        return []

    def load_depends(self, ls):
        """ ensure that all modules indicates in the module dependencies are well loaded.
        Returns list of diagnostics to publish in manifest file """
        diagnostics = []
        for depend in self.depends:
            if depend in OdooModule.modules:
                OdooModule.modules[depend].load(ls)
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
    
    def load_data(self):
        return []

    def load_python_files(self, ls, path):
        #if self.path in server.odooBase.OdooBase.files:
        #    if not rebuild:
        #        return
        #    else:
        #        #TODO remove data ??????? o
        #        pass
        parser = PythonParser(ls, path, odooBase.OdooBase.get().symbols.get_symbol(["odoo", "addons"]))
        parser.load_symbols()
        return []
