import ast
import asyncio
import os
import parso
import re
import sys
import traceback
import threading
from ..constants import *
from .symbol import *
from .fileMgr import *
from .threadCondition import ReadWriteCondition
from contextlib import contextmanager
from lsprotocol.types import (ConfigurationItem, WorkspaceConfigurationParams)

from ..constants import CONFIGURATION_SECTION

#for debug
import time

import tracemalloc


class Odoo():

    odooPath = ""

    version_major = 0
    version_minor = 0
    version_micro = 0

    grammar = None

    fileMgr = FileMgr()

    models = {} 
    modules = {}

    # symbols is the list of declared symbols and their related declaration, filtered by name
    symbols = RootSymbol("root", SymType.ROOT, [])

    to_rebuild = [] # list of symbols (ref) to rebuild at arch level. see add_to_arch_rebuild
    to_init_odoo = weakref.WeakSet() # Set of symbols that need a refresh of Odoo data
    to_validate = weakref.WeakSet() # Set of symbols that need to be revalidated

    not_found_symbols = weakref.WeakSet() # Set of symbols that still have unresolved dependencies

    instance = None

    write_lock = threading.Lock()
    thread_access_condition = ReadWriteCondition(10) #should match the number of threads

    import_odoo_addons = True #can be set to False for speed up tests

    def __init__(self):
        pass

    @contextmanager
    def acquire_write(self, ls):
        with self.write_lock:
            ls.send_notification('Odoo/loadingStatusUpdate', 'start')
            print("Odoo/loading: start")
            self.thread_access_condition.wait_empty()
            yield
            ls.send_notification('Odoo/loadingStatusUpdate', 'stop')
            print("Odoo/loading: stop")

    @contextmanager
    def acquire_read(self):
        with self.write_lock:
            self.thread_access_condition.acquire()
        yield
        self.thread_access_condition.release()


    @staticmethod
    def get(ls = None):
        if not Odoo.instance:
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
                with Odoo.instance.acquire_write(ls):
                    Odoo.instance.symbols.paths = []
                    for path in sys.path:
                        if os.path.isdir(path):
                            Odoo.instance.symbols.paths.append(path)
                    Odoo.instance.grammar = parso.load_grammar(version="3.8") #TODO config or choose automatically
                    Odoo.instance.start_build_time = time.time()
                    Odoo.instance.odooPath = config[0]['userDefinedConfigurations'][str(config[0]['selectedConfigurations'])]['odooPath']
                    Odoo.instance.build_database(ls, config[0]['userDefinedConfigurations'][str(config[0]['selectedConfigurations'])])
                    print("End building database in " + str(time.time() - Odoo.instance.start_build_time) + " seconds")
            except Exception as e:
                print(traceback.format_exc())
                ls.show_message_log(f'Error ocurred: {e}')
        return Odoo.instance
    
    def get_symbol(self, fileTree, nameTree = []):
        return self.symbols.get_symbol(fileTree, nameTree)

    def build_database(self, ls, used_config):
        if not self.build_base(ls, used_config):
            return False
        self.build_modules(ls)

    def build_base(self, ls, used_config):
        from .pythonArchBuilder import PythonArchBuilder
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
                if self.version_major < 14:
                    ls.show_message("Odoo version is too old. The tool only supports version 14 and above.")
            #set python path
            self.symbols.paths += [self.odooPath]
            parser = PythonArchBuilder(ls, self.symbols, os.path.join(self.odooPath, "odoo"))
            parser.load_arch()
            self.process_odoo_init(ls)
            self.process_validations(ls)
            addonsSymbol = self.symbols.get_symbol(["odoo", 'addons'])
            if Odoo.import_odoo_addons:
                addonsSymbol.paths += [
                    os.path.join(self.odooPath, "addons"), 
                    #"/home/odoo/Documents/odoo-servers/false_odoo/enterprise"
                    ]
            addonsSymbol.paths += used_config['addons']
            return True
        else:
            print("Odoo not found at " + self.odooPath)
            return False
        return False

    def process_arch_rebuild(self, ls):
        from .pythonArchBuilder import PythonArchBuilder
        print("rebuild " + str(len(self.to_rebuild)))
        already_rebuilt = set()
        while self.to_rebuild:
            symbol_ref = self.to_rebuild.pop()
            symbol = symbol_ref()
            if not symbol:
                continue
            print("triggering arch rebuild of " + symbol.name + " from " + symbol.paths[0])
            tree = symbol.get_tree()
            tree = (tuple(tree[0]), tuple(tree[1])) #make it hashable
            if tree in already_rebuilt:
                continue #TODO cyclic dependency
            already_rebuilt.add(tree)
            parent = symbol.parent
            ast_node = symbol.ast_node()
            #WRONG, the context of the stacktrace will prevent ANY deletion, and making it buggy
            symbol.unload(symbol)
            del symbol
            #build new
            if parent and ast_node:
                pp = PythonArchBuilder(ls, parent, ast_node).load_arch()
            else:
                print("Can't rebuild " + str(tree))

    def process_odoo_init(self, ls):
        from .pythonOdooBuilder import PythonOdooBuilder
        print("init " + str(len(self.to_init_odoo)))
        for symbol in self.to_init_odoo:
            validation = PythonOdooBuilder(ls, symbol)
            validation.load_odoo_content()
        self.to_init_odoo.clear()

    def process_validations(self, ls):
        from server.features.validation.pythonValidator import PythonValidator
        print("validating " + str(len(self.to_validate)))
        for symbol in self.to_validate:
            validation = PythonValidator(ls, symbol)
            validation.validate()
        self.to_validate.clear()

    def build_modules(self, ls):
        from .module import Module
        addonPaths = self.symbols.get_symbol(["odoo", "addons"]).paths
        for path in addonPaths:
            dirs = os.listdir(path)
            for dir in dirs:
                Module(ls, os.path.join(path, dir))
        if FULL_LOAD_AT_STARTUP:
            for module in Odoo.get().modules.values():
                module.load_arch(ls)
            print("start odoo loading")
            self.process_odoo_init(ls)
            print("start validation")
            self.process_validations(ls) #Maybe avoid this as the weakset can be quite big?

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
                return self.symbols.get_symbol(["odoo", "addons"] + path.replace(".py", "")[len(addonPath)+1:].replace("\\", "/").split("/"))
        return []

    def file_change(self, ls, path, text, version):
        from .pythonArchBuilder import PythonArchBuilder

        #snapshot1 = tracemalloc.take_snapshot()
        if path.endswith(".py"):
            print("reload triggered on " + path + " version " + str(version))
            file_info = FileMgr.getFileInfo(path, text, version)
            if not file_info["ast"]:
                return #could emit syntax error in file_info["d_synt"]
            with Odoo.get(ls).acquire_write(ls):
                #1 unload
                if path.endswith("__init__.py"):
                    path = os.sep.join(path.split(os.sep)[:-1])
                file_symbol = self.get_file_symbol(path)
                parent = file_symbol.parent
                file_symbol.unload(file_symbol)
                del file_symbol
                #build new
                pp = PythonArchBuilder(ls, parent, path)
                new_symbol = pp.load_arch()
                #rebuild validations
                self.process_arch_rebuild(ls)
                self.process_odoo_init(ls)
                self.process_validations(ls)
                if new_symbol:
                    set_to_validate = self._search_symbols_to_revalidate(new_symbol.get_tree())
                    self.validate_related_files(ls, set_to_validate)
        #snapshot2 = tracemalloc.take_snapshot()

        #top_stats = snapshot2.compare_to(snapshot1, 'lineno')
        return
    
    def file_rename(self, ls, old_path, new_path):
        from server.core.pythonArchBuilder import PythonArchBuilder
        with Odoo.get(ls).acquire_write(ls):
            #unload old
            file_symbol = self.get_file_symbol(old_path)
            if file_symbol:
                file_symbol.unload(file_symbol)
            #build new
            parent_path = os.sep.join(new_path.split(os.sep)[:-1])
            parent_symbol = self.get_file_symbol(parent_path)
            new_symbol = None
            if not parent_symbol:
                print("parent symbol not found: " + parent_path)
            else:
                print("found: " + str(parent_symbol.get_tree()))
                new_tree = parent_symbol.get_tree()
                new_tree[1].append(new_path.split(os.sep)[-1].replace(".py", ""))
                set_to_validate = self._search_symbols_to_revalidate(new_tree)
                if set_to_validate:
                    #if there is something that is trying to import the new file, build it.
                    #Else, don't add it to the architecture to not add useless symbols (and overrides)
                    if new_path.endswith("__init__.py"):
                        new_path = os.sep.join(new_path.split(os.sep)[:-1])
                    pp = PythonArchBuilder(ls, parent_symbol, new_path)
                    del file_symbol
                    new_symbol = pp.load_arch()
            #rebuild validations
            self.process_arch_rebuild(ls)
            self.process_odoo_init(ls)
            self.process_validations(ls)
            if new_symbol:
                self.validate_related_files(ls, set_to_validate)

    def add_to_arch_rebuild(self, symbol):
        """ add a symbol to the list of arch rebuild to do."""
        if symbol:
            self.to_rebuild.append(weakref.ref(symbol))

    def add_to_init_odoo(self, symbol, force=False):
        """ add a symbol to the list of odoo loading to do. if Force, the symbol will be added even if
        he is already validated"""
        if symbol:
            file = symbol.get_in_parents([SymType.FILE, SymType.PACKAGE, SymType.NAMESPACE])
            if not file:
                print("file not found, can't rebuild")
                return
            if force:
                file.odooStatus = 0
                file.validationStatus = 0
            self.to_init_odoo.add(file)

    def add_to_validations(self, symbol, force=False):
        """ add a symbol to the list of revalidation to do. if Force, the symbol will be added even if
        he is already validated"""
        if symbol:
            file = symbol.get_in_parents([SymType.FILE, SymType.PACKAGE, SymType.NAMESPACE])
            if not file:
                print("file not found, can't rebuild")
                return
            if force:
                file.validationStatus = 0
            self.to_validate.add(file)

    def _search_symbols_to_revalidate(self, tree):
        flat_tree = [item for l in tree for item in l]
        new_set_to_revalidate = weakref.WeakSet()
        for s in self.not_found_symbols:
            for p in s.not_found_paths:
                if flat_tree[:len(p)] == p[:len(flat_tree)]: #TODO wrong
                    new_set_to_revalidate.add(s)
                    print("found one pending: " + str(s.get_tree()))
        return new_set_to_revalidate
    
    def validate_related_files(self, ls, set_to_validate):
        from server.features.validation.pythonValidator import PythonValidator
        from .pythonOdooBuilder import PythonOdooBuilder
        for s in set_to_validate:
            s.odooStatus = 0
            s.validationStatus = 0
            PythonOdooBuilder(ls, s).load_odoo_content()
            PythonValidator(ls, s).validate()

    def get_models(self, module = None, start_name = ""):
        res = []
        for name, model in self.models.items():
            if name.startswith(start_name):
                if module:
                    if model.get_main_symbols(module):
                        res += [model]
                else:
                    res += [model]
        return res
