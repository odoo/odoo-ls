import ast
import os
import parso
import re
import sys
import traceback
from .constants import *
from .symbol import *
from .fileMgr import *
from lsprotocol.types import (CompletionItem, CompletionList, CompletionOptions,
                             CompletionParams, ConfigurationItem,
                             ConfigurationParams, Diagnostic,
                             DidChangeTextDocumentParams,
                             DidCloseTextDocumentParams,
                             DidOpenTextDocumentParams, MessageType, Position,
                             Range, Registration, RegistrationParams,
                             SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
                             Unregistration, UnregistrationParams, WorkspaceConfigurationParams)

from .constants import CONFIGURATION_SECTION

#for debug
import time


class Odoo():

    odooPath = ""
    isLoading = False

    version_major = 0
    version_minor = 0
    version_micro = 0

    grammar = None

    fileMgr = FileMgr()

    # for each model, the list of symbols implementing it
    # models = {
    # "account.test": Model}
    models = {} 
    modules = {}

    # symbols is the list of declared symbols and their related declaration, filtered by name
    symbols = Symbol("root", "root", [])

    to_rebuild = {} #by files, symbol tree to rebuild

    instance = None

    def __init__(self):
        pass

    @staticmethod
    def get(ls = None):
        if not Odoo.instance:
            Odoo.isLoading = True
            if not ls:
                print(f"Can't initialize Odoo Base : No odoo server provided. Please contact support.")
            
            print("Creating new Odoo Base")

            try:
                config = ls.get_configuration(WorkspaceConfigurationParams(items=[
                    ConfigurationItem(
                        scope_uri='userDefinedConfigurations',
                        section=CONFIGURATION_SECTION)
                ])).result()
                Odoo.instance = Odoo()
                Odoo.instance.grammar = parso.load_grammar(version="3.8") #config or choose automatically
                Odoo.instance.start_build_time = time.time()
                Odoo.instance.odooPath = config[0]['userDefinedConfigurations'][str(config[0]['selectedConfigurations'])]['odooPath']
                Odoo.instance.build_database(ls)
                print("End building database in " + str(time.time() - Odoo.instance.start_build_time) + " seconds")
            except Exception as e:
                print(traceback.format_exc())
                ls.show_message_log(f'Error ocurred: {e}')
            Odoo.isLoading = False
        return Odoo.instance
    
    def build_database(self, ls):
        if not self.build_base(ls):
            return False
        self.build_modules(ls)

    def build_base(self, ls):
        from server.pythonArchBuilder import PythonArchBuilder
        releasePath = os.path.join(self.odooPath, "odoo", "release.py")
        if os.path.exists(releasePath):
            with open(releasePath, "r") as f:
                lines = f.readlines()
                for line in lines:
                    if line.startswith("version_info ="):
                        reg = r"version_info = \((\d+, \d+, \d+, \w+, \d+, \D+)\)"
                        match = re.match(reg, line)
                        res = match.group(1).split(", ")
                        self.version_major = int(res[0])
                        self.version_minor = int(res[1])
                        self.version_micro = int(res[2])
                print(f"Odoo version: {self.version_major}.{self.version_minor}.{self.version_micro}")
            #set python path
            self.symbols.paths += [self.odooPath]
            parser = PythonArchBuilder(ls, os.path.join(self.odooPath, "odoo"), self.symbols.get_symbol([]))
            parser.load_arch()
            addonsSymbol = self.symbols.get_symbol(["odoo", 'addons'])
            addonsSymbol.paths += [
                os.path.join(self.odooPath, "addons"), 
                #"/home/odoo/Documents/odoo-servers/false_odoo/enterprise"
                ]
            return True
        else:
            print("Odoo not found at " + self.odooPath)
            return False
        return False

    def build_modules(self, ls):
        from server.module import Module
        addonPaths = self.symbols.get_symbol(["odoo", "addons"]).paths
        for path in addonPaths:
            dirs = os.listdir(path)
            for dir in dirs:
                Module(ls, os.path.join(path, dir))
        if FULL_LOAD_AT_STARTUP:
            for module in Odoo.get().modules.values():
                module.load(ls)


        try:
            import psutil
            print("ram usage : " + str(psutil.Process(os.getpid()).memory_info().rss / 1024 ** 2) + " Mo")
        except Exception:
            print("psutil not found")
            pass
        print(str(len(Odoo.get().modules)) + " modules found")
    
    def get_file_symbol(self, path):
        if path.startswith(self.instance.odooPath):
            tree = path.replace(".py", "")[len(self.instance.odooPath)+1:].replace("\\", "/").split("/")
            if tree:
                if tree[0] == "addons":
                    tree = ["odoo"] + tree
            return self.symbols.get_symbol(tree)
        for addonPath in self.symbols.get_symbol(["odoo", "addons"]).paths:
            if path.startswith(addonPath):
                return self.symbols.get_symbol(["odoo", "addons"] + path.replace(".py", "")[len(addonPath)+1:].split("/"))
        return []

    def file_change(self, ls, path, text, version):
        from server.pythonArchBuilder import PythonArchBuilder
        if path.endswith(".py"):
            file_symbol = self.get_file_symbol(path)
            pp = PythonArchBuilder(ls, path, file_symbol.parent)
            pp.load_arch(text, version)

    def add_to_rebuild(self, symbol_path):
        """ add a symbol (with its tree path) to the list of rebuild to do."""
        index = 0
        while index != len(self.to_rebuild):
            s = self.to_rebuild[s]
            if len(s) < len(symbol_path):
                if s == symbol_path[:len(s)]:
                    return
            elif symbol_path == s[:len(symbol_path)]:
                del self.to_rebuild[index]
                index -=1
            index += 1
        self.to_rebuild.append(symbol_path)
