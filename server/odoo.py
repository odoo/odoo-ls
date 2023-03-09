import ast
import asyncio
import os
import parso
import re
import sys
import traceback
import threading
from .constants import *
from .symbol import *
from .fileMgr import *
from .threadCondition import ReadWriteCondition
from contextlib import contextmanager
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
    symbols = RootSymbol("root", "root", [])

    to_rebuild = weakref.WeakSet()

    not_found_symbols = weakref.WeakSet() # Set of symbols that still have unresolved dependencies

    instance = None

    write_lock = threading.Lock()
    thread_access_condition = ReadWriteCondition(10) #should match the number of threads

    def __init__(self):
        pass

    @contextmanager
    def acquire_write(self):
        with self.write_lock:
            self.thread_access_condition.wait_empty()
            yield

    @contextmanager
    def acquire_read(self):
        with self.write_lock:
            self.thread_access_condition.acquire()
        yield
        self.thread_access_condition.release()


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
                Odoo.instance.symbols.paths = []
                for path in sys.path:
                    if os.path.isdir(path):
                        Odoo.instance.symbols.paths.append(path)
                Odoo.instance.grammar = parso.load_grammar(version="3.8") #TODO config or choose automatically
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
        from server.pythonValidator import PythonValidator
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
            parser = PythonArchBuilder(ls, os.path.join(self.odooPath, "odoo"), self.symbols)
            parser.load_arch()
            validation = PythonValidator(ls, self.symbols.get_symbol(["odoo"]))
            validation.validate()
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
                module.load_arch(ls)
            for module in Odoo.get().modules.values():
                module.validate(ls)


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
                if tree[-1] == "__init__":
                    tree.pop()
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
            print("reload triggered on " + path + " version " + str(version))
            file_info = FileMgr.getFileInfo(path, text, version)
            if not file_info["ast"]:
                return #could emit syntax error in file_info["d_synt"]
            with Odoo.get().acquire_write():
                #1 unload
                file_symbol = self.get_file_symbol(path)
                file_symbol.unload()
                #build new
                pp = PythonArchBuilder(ls, path, file_symbol.parent)
                del file_symbol
                new_symbol = pp.load_arch()
                #rebuild validations
                self.rebuild_validations(ls)
                if new_symbol:
                    set_to_validate = self._search_symbols_to_rebuild(new_symbol.get_tree())
                    self.validate_related_files(ls, set_to_validate)
    
    def file_rename(self, ls, old_path, new_path):
        from server.pythonArchBuilder import PythonArchBuilder
        with Odoo.get().acquire_write():
            #unload old
            file_symbol = self.get_file_symbol(old_path)
            if file_symbol:
                file_symbol.unload()
            #build new
            parent_path = "/".join(new_path.split("/")[:-1])
            parent_symbol = self.get_file_symbol(parent_path)
            new_symbol = None
            if not parent_symbol:
                print("parent symbol not found: " + parent_path)
            else:
                print("found: " + str(parent_symbol.get_tree()))
                new_tree = parent_symbol.get_tree()
                new_tree[1].append(new_path.split("/")[-1].replace(".py", ""))
                set_to_validate = self._search_symbols_to_rebuild(new_tree)
                if set_to_validate:
                    #if there is something that is trying to import the new file, build it.
                    #Else, don't add it to the architecture to not add useless symbols (and overrides)
                    pp = PythonArchBuilder(ls, new_path, parent_symbol)
                    del file_symbol
                    new_symbol = pp.load_arch()
            #rebuild validations
            self.rebuild_validations(ls)
            if new_symbol:
                self.validate_related_files(ls, set_to_validate)

    def add_to_rebuild(self, symbol):
        """ add a symbol to the list of rebuild to do."""
        if symbol:
            file = symbol.get_in_parents(["file", "package", "namespace"])
            if not file:
                print("file not found, can't rebuild")
                return
            file.validationStatus = 0
            self.to_rebuild.add(file)

    def rebuild_validations(self, ls):
        """ Rebuild validation of all pending files. Be sure to have a write lock """
        from server.pythonValidator import PythonValidator
        for file in self.to_rebuild:
            print(file.paths[0])
            PythonValidator(ls, file).validate()
        self.to_rebuild.clear()

    def _search_symbols_to_rebuild(self, tree):
        flat_tree = [item for l in tree for item in l]
        new_set_to_revalidate = weakref.WeakSet()
        for s in self.not_found_symbols:
            for p in s.not_found_paths:
                if flat_tree[:len(p)] == p[:len(flat_tree)]:
                    new_set_to_revalidate.add(s)
                    print("found one pending: " + str(s.get_tree()))
        return new_set_to_revalidate
    
    def validate_related_files(self, ls, set_to_validate):
        from server.pythonValidator import PythonValidator
        for s in set_to_validate:
            s.validationStatus = 0
            PythonValidator(ls, s).validate()

