import ast
import os
import re
import sys
import traceback
from .constants import *
from pygls.lsp.types import (CompletionItem, CompletionList, CompletionOptions,
                             CompletionParams, ConfigurationItem,
                             ConfigurationParams, Diagnostic,
                             DidChangeTextDocumentParams,
                             DidCloseTextDocumentParams,
                             DidOpenTextDocumentParams, MessageType, Position,
                             Range, Registration, RegistrationParams,
                             SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
                             Unregistration, UnregistrationParams)

from .constants import CONFIGURATION_SECTION

#for debug
import time

class Symbol():

    def __init__(self, name, type, paths):
        self.name = name
        self.type = type
        self.paths = paths if isinstance(paths, list) else [paths]
        self.symbols = {}
        self.parent = None
        self.modelName = None
        self.bases = [] #for class only
        #local namespace and corresponding symbol
        # {odoo.addons: Symbol}
        self.localAliases = {}
        self.startLine = 0
        self.endLine = 0
    
    def __str__(self):
        return "(" + self.name + " - " + self.type + " - " + str(self.paths) + ")"

    def get_tree(self):
        ancestors = []
        curr_symbol = self
        while curr_symbol.parent and curr_symbol.parent.type != "root":
            ancestors.insert(0, curr_symbol.parent.name)
            curr_symbol = curr_symbol.parent
        return ancestors + [self.name]

    def get_symbol(self, symbol_names):
        if not symbol_names:
            return self
        if symbol_names[0] in self.localAliases:
            curr_symbol = self.localAliases[symbol_names[0]]
            if curr_symbol:
                return curr_symbol.get_symbol(symbol_names[1:])
        if symbol_names[0] in self.symbols:
            curr_symbol = self.symbols[symbol_names[0]]
            if curr_symbol:
                return curr_symbol.get_symbol(symbol_names[1:])
        return False

    def add_symbol(self, symbol_names, symbol):
        """take a list of symbols name representing a relative path (ex: odoo.addon.models) and the symbol to add"""
        if symbol_names and symbol_names[0] not in self.symbols:
            raise Exception("Symbol not found: " + str(symbol_names[0]))
        curr_symbol = self.symbols[symbol_names[0]] if symbol_names else self
        for s in symbol_names[1:]:
            if s in curr_symbol.symbols:
                curr_symbol = curr_symbol.symbols[s]
            else:
                raise Exception("Package not found: " + str(symbol_names))
        symbol.parent = curr_symbol
        curr_symbol.symbols.update({symbol.name: symbol})

    def get_scope_symbol(self, line):
        """return the symbol (class or function) the closest to the given line """
        symbol = self
        for s in self.symbols.values():
            if s.startLine <= line and s.endLine >= line:
                symbol = s.get_scope_symbol(line)
                break
        return symbol

class Model():

    def __init__(self, name):
        self.name = name
        self.inherit = []
        self.inherited_by = []
        self.symbols = []



class OdooBase():

    odooPath = ""

    version_major = 0
    version_minor = 0
    version_micro = 0

    # for each model, the list of symbols implenting it
    # models = {
    # "account.test": Model}
    models = {} 
    # for each file path: a reference to the symbol of this file you can find in self.symbols
    files = {}

    # symbols is the list of declared symbols and their related declaration, filtered by name
    symbols = Symbol("root", "root", [])

    instance = None

    def __init__(self):
        pass

    @staticmethod
    def get(ls = None):
        if not OdooBase.instance:
            if not ls:
                ls.show_message_log(f"Can't initialize Odoo Base : No odoo server provided. Please contact support.")
            
            print("Creating new Odoo Base")

            try:
                config = ls.get_configuration(ConfigurationParams(items=[
                    ConfigurationItem(
                        scope_uri='userDefinedConfigurations',
                        section=CONFIGURATION_SECTION)
                ])).result()
                OdooBase.instance = OdooBase()
                OdooBase.instance.start_build_time = time.time()
                OdooBase.instance.odooPath = config[0]['userDefinedConfigurations'][str(config[0]['selectedConfigurations'])]['odooPath']
                OdooBase.instance.build_database(ls)
                print("End building database in " + str(time.time() - OdooBase.instance.start_build_time) + " seconds")
            except Exception as e:
                print(traceback.format_exc())
                ls.show_message_log(f'Error ocurred: {e}')
        return OdooBase.instance
    
    def build_database(self, ls):
        if not self.build_base(ls):
            return False
        self.build_modules(ls)

    def build_base(self, ls):
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
            self.symbols.add_symbol([], Symbol("odoo", "package", [self.odooPath]))
            self.symbols.add_symbol(["odoo"], Symbol("addons", "package", [self.odooPath + "/odoo/addons", self.odooPath + "/addons", "/home/odoo/Documents/odoo-servers/false_odoo/enterprise"]))
            return True
        else:
            print("Odoo not found at " + self.odooPath)
            return False
        return False

    def build_modules(self, ls):
        from server.odooModule import OdooModule
        addonPaths = self.symbols.get_symbol(["odoo", "addons"]).paths
        for path in addonPaths:
            dirs = os.listdir(path)
            for dir in dirs:
                OdooModule(ls, os.path.join(path, dir))
        if FULL_LOAD_AT_STARTUP:
            for module in OdooModule.modules.values():
                module.load(ls)


        try:
            import psutil
            print("ram usage : " + str(psutil.Process(os.getpid()).memory_info().rss / 1024 ** 2) + " Mo")
        except Exception:
            print("psutil not found")
            pass
        print(str(len(OdooModule.modules)) + " modules found")
    
    def get_file_symbol(self, uri):
        if uri in self.files:
            return self.files[uri]
        return []

    def init_file(uri):
        if uri.endswith(".py"):
            pass