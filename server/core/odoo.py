import ast
import asyncio
import os
import parso
import pathlib
import re
import sys
import traceback
import threading
from ..constants import *
from .symbol import *
from .fileMgr import *
from .threadCondition import ReadWriteCondition
from server.references import RegisteredRefSet
from contextlib import contextmanager
from lsprotocol.types import (ConfigurationItem, WorkspaceConfigurationParams)
from pygls.server import LanguageServer, MessageType

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

    rebuild_arch = RegisteredRefSet()
    rebuild_arch_eval = RegisteredRefSet()
    rebuild_odoo = RegisteredRefSet()
    rebuild_validation = RegisteredRefSet()

    not_found_symbols = RegisteredRefSet() # Set of symbols that still have unresolved dependencies (arch level only)

    instance = None

    write_lock = threading.Lock()
    thread_access_condition = ReadWriteCondition(10) #should match the number of threads

    import_odoo_addons = True #can be set to False for speed up tests

    stubs_dir = os.path.join(pathlib.Path(__file__).parent.parent.resolve(), "typeshed", "stubs")

    def __init__(self):
        pass

    @contextmanager
    def acquire_write(self, ls):
        with self.write_lock:
            ls.send_notification('Odoo/loadingStatusUpdate', 'start')
            self.thread_access_condition.wait_empty()
            context = threading.local()
            context.lock_type = "write"
            yield
            context.lock_type = "none"
            ls.send_notification('Odoo/loadingStatusUpdate', 'stop')

    @contextmanager
    def acquire_read(self):
        with self.write_lock:
            self.thread_access_condition.acquire()
        context = threading.local()
        context.lock_type = "read"
        yield
        self.thread_access_condition.release()
        context.lock_type = "none"

    @contextmanager
    def upgrade_to_write(self):
        if threading.local().lock_type != "read":
            raise Exception("Can't upgrade to write from a non read lock")
        self.thread_access_condition.release()
        with self.acquire_write():
            yield
        with self.write_lock:
            self.thread_access_condition.acquire()

    @staticmethod
    def get(ls:LanguageServer = None):
        if not Odoo.instance:
            if not ls:
                print(f"Can't initialize Odoo Base : No odoo server provided. Please contact support.")
            ls.show_message_log("Building new Odoo knowledge database")

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
                    # add stubs for not installed packages
                    Odoo.instance.symbols.paths.append(Odoo.stubs_dir)
                    Odoo.instance.symbols.paths.append(os.path.join(pathlib.Path(__file__).parent.parent.resolve(), "typeshed", "stdlib"))
                    Odoo.instance.grammar = parso.load_grammar(version="3.8") #TODO config or choose automatically
                    Odoo.instance.start_build_time = time.time()
                    Odoo.instance.odooPath = config[0]['userDefinedConfigurations'][str(config[0]['selectedConfigurations'])]['odooPath']
                    Odoo.instance.build_database(ls, config[0]['userDefinedConfigurations'][str(config[0]['selectedConfigurations'])])
                    ls.show_message_log("End building database in " + str(time.time() - Odoo.instance.start_build_time) + " seconds")
            except Exception as e:
                ls.show_message_log(traceback.format_exc())
                ls.show_message_log(f'Error ocurred: {e}', MessageType.Error)
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
                ls.show_message_log(f"Odoo version: {self.version_major}.{self.version_minor}.{self.version_micro}")
                if self.version_major < 14:
                    ls.show_message("Odoo version is too old. The tool only supports version 14 and above.", MessageType.Error)
            #set python path
            self.symbols.paths += [self.odooPath]
            parser = PythonArchBuilder(ls, self.symbols, os.path.join(self.odooPath, "odoo"))
            parser.load_arch()
            self.process_rebuilds(ls)
            addonsSymbol = self.symbols.get_symbol(["odoo", 'addons'])
            if Odoo.import_odoo_addons:
                addonsSymbol.paths += [
                    os.path.join(self.odooPath, "addons"),
                    #"/home/odoo/Documents/odoo-servers/test_odoo/enterprise",
                    ]
            addonsSymbol.paths += used_config['addons']
            return True
        else:
            ls.show_message_log("Odoo not found at " + self.odooPath, MessageType.Error)
            return False
        return False

    def _pop_from_list(self, sym_set):
            selected_sym = None
            selected_count = 1000000
            for sym in sym_set:
                current_count = 0
                for dep_level, sym_dep_set in sym.dependencies[BuildSteps.ARCH].items():
                    if dep_level == BuildSteps.ARCH:
                        for dep in sym_dep_set:
                            if dep in self.rebuild_arch:
                                current_count += 1
                    elif dep_level == BuildSteps.ARCH_EVAL:
                        for dep in sym_dep_set:
                            if dep in self.rebuild_arch_eval:
                                current_count += 1
                    elif dep_level == BuildSteps.ODOO:
                        for dep in sym_dep_set:
                            if dep in self.rebuild_odoo:
                                current_count += 1
                    elif dep_level == BuildSteps.VALIDATION:
                        for dep in sym_dep_set:
                            if dep in self.rebuild_validation:
                                current_count += 1
                if current_count < selected_count:
                    selected_sym = sym
                    selected_count = current_count
                if selected_count == 0:
                    break
            sym_set.remove(selected_sym)
            return selected_sym

    def _pop_next_symbol(self, level):
        #pop the next symbol ready to be rebuilt, depending on its dependencies
        if level == BuildSteps.ARCH:
            return self._pop_from_list(self.rebuild_arch)
        elif level == BuildSteps.ARCH_EVAL:
            return self._pop_from_list(self.rebuild_arch_eval)
        elif level == BuildSteps.ODOO:
            return self._pop_from_list(self.rebuild_odoo)
        elif level == BuildSteps.VALIDATION:
            return self._pop_from_list(self.rebuild_validation)

    def process_rebuilds(self, ls):
        from .pythonArchBuilder import PythonArchBuilder
        from .pythonArchEval import PythonArchEval
        from .pythonOdooBuilder import PythonOdooBuilder
        from server.features.validation.pythonValidator import PythonValidator
        if DEBUG_REBUILD:
            ls.show_message_log("starting rebuild process")
            arch_rebuilt = []
            eval_rebuilt = []
            odoo_rebuilt = []
            validation_rebuilt = []
        already_arch_rebuilt = set()
        while True:
            if self.rebuild_arch:
                sym = self._pop_next_symbol(BuildSteps.ARCH)
                if not sym:
                    continue
                tree = sym.get_tree()
                tree = (tuple(tree[0]), tuple(tree[1])) #make it hashable
                if tree in already_arch_rebuilt: #prevent cyclic imports infinite loop
                    if DEBUG_REBUILD:
                        print("arch rebuild skipped - already rebuilt")
                    continue
                if DEBUG_REBUILD:
                    arch_rebuilt.append(tree)
                already_arch_rebuilt.add(tree)
                parent = sym.parent
                ast_node = sym.ast_node
                path = sym.paths[0]
                sym.unload(sym)
                del sym
                #build new
                if parent and ast_node:
                    pp = PythonArchBuilder(ls, parent, path, ast_node).load_arch()
                elif DEBUG_REBUILD:
                    ls.show_message_log("Can't rebuild " + str(tree))
                continue
            elif self.rebuild_arch_eval:
                if DEBUG_REBUILD and arch_rebuilt:
                    ls.show_message_log("Arch rebuilt done for " + "\n".join([str(t) for t in arch_rebuilt]))
                    arch_rebuilt = []
                sym = self._pop_next_symbol(BuildSteps.ARCH_EVAL)
                if not sym or sym.type == SymType.DIRTY:
                    continue
                if DEBUG_REBUILD:
                    eval_rebuilt.append(sym.get_tree())
                evaluator = PythonArchEval(ls, sym)
                evaluator.eval_arch()
                continue
            elif self.rebuild_odoo:
                if DEBUG_REBUILD and eval_rebuilt:
                    ls.show_message_log("Eval rebuilt done for " + "\n".join([str(t) for t in eval_rebuilt]))
                    eval_rebuilt = []
                sym = self._pop_next_symbol(BuildSteps.ODOO)
                if not sym:
                    continue
                if DEBUG_REBUILD:
                    odoo_rebuilt.append(sym.get_tree())
                validation = PythonOdooBuilder(ls, sym)
                validation.load_odoo_content()
                continue
            elif self.rebuild_validation:
                if DEBUG_REBUILD and odoo_rebuilt:
                    ls.show_message_log("Odoo rebuilt done for " + "\n".join([str(t) for t in odoo_rebuilt]))
                    odoo_rebuilt = []
                sym = self._pop_next_symbol(BuildSteps.VALIDATION)
                if not sym:
                    continue
                if DEBUG_REBUILD:
                    validation_rebuilt.append(sym.get_tree())
                validation = PythonValidator(ls, sym)
                validation.validate()
                continue
            break
        if DEBUG_REBUILD and validation_rebuilt:
            ls.show_message_log("Validation rebuilt done for " + "\n".join([str(t) for t in validation_rebuilt]))
            validation_rebuilt = []

    def build_modules(self, ls):
        from .module import Module
        addonPaths = self.symbols.get_symbol(["odoo", "addons"]).paths
        for path in addonPaths:
            dirs = os.listdir(path)
            for dir in dirs:
                Module(ls, os.path.join(path, dir))
        loaded = []
        if not DEBUG_BUILD_ONLY_BASE:
            for module in Odoo.get().modules.values():
                loaded += module.load_arch(ls)
            self.process_rebuilds(ls)

        if loaded:
            ls.show_message_log("Modules loaded: " + ", ".join(loaded))

        try:
            import psutil
            ls.show_message_log("ram usage : " + str(psutil.Process(os.getpid()).memory_info().rss / 1024 ** 2) + " Mo")
        except Exception:
            ls.show_message_log("ram usage: unknown (please install psutil to get ram usage)")
            pass
        ls.show_message_log(str(len(Odoo.get().modules)) + " modules found")

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
            ls.show_message_log("File change event: " + path + " version " + str(version))
            file_info = FileMgr.getFileInfo(path, text, version, opened=True)
            FileMgr.publish_diagnostics(ls, file_info)
            if not file_info["ast"]:
                return #could emit syntax error in file_info["d_synt"]
            with Odoo.get(ls).acquire_write(ls):
                #1 unload
                if path.endswith("__init__.py") or path.endswith("__init__.pyi"):
                    path = os.sep.join(path.split(os.sep)[:-1])
                file_symbol = self.get_file_symbol(path)
                parent = file_symbol.parent
                file_symbol.unload(file_symbol)
                del file_symbol
                #build new
                pp = PythonArchBuilder(ls, parent, path)
                new_symbol = pp.load_arch()
                new_symbol_tree = new_symbol.get_tree()
                del new_symbol
                #rebuild validations
                if new_symbol_tree:
                    set_to_validate = self._search_symbols_to_rebuild(new_symbol_tree)
                    for s in set_to_validate:
                        self.add_to_arch_rebuild(s)
                self.process_rebuilds(ls)
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
            del file_symbol
            #build new
            parent_path = os.sep.join(new_path.split(os.sep)[:-1])
            parent_symbol = self.get_file_symbol(parent_path)
            new_symbol = None
            if not parent_symbol:
                ls.show_message_log("parent symbol not found: " + parent_path, MessageType.Error)
                ls.show_message("Unable to rename file. Internal representation is not right anymore", 1)
            else:
                new_tree = parent_symbol.get_tree()
                new_tree[1].append(new_path.split(os.sep)[-1].replace(".py", ""))
                set_to_validate = self._search_symbols_to_rebuild(new_tree)
                if set_to_validate:
                    #if there is something that is trying to import the new file, build it.
                    #Else, don't add it to the architecture to not add useless symbols (and overrides)
                    if new_path.endswith("__init__.py") or new_path.endswith("__init__.pyi"):
                        new_path = os.sep.join(new_path.split(os.sep)[:-1])
                    pp = PythonArchBuilder(ls, parent_symbol, new_path)
                    new_symbol = pp.load_arch()
            #rebuild validations
            if new_symbol:
                for s in set_to_validate:
                    self.add_to_arch_rebuild(s)
            self.process_rebuilds(ls)

    def add_to_arch_rebuild(self, symbol):
        """ add a symbol to the list of arch rebuild to do."""
        if symbol:
            #print("add to arch rebuild: " + str(symbol.get_tree()))
            symbol.archStatus = 0
            symbol.evalStatus = 0
            symbol.odooStatus = 0
            symbol.validationStatus = 0
            self.rebuild_arch.add(symbol)

    def add_to_arch_eval(self, symbol):
        """ add a symbol to the list of arch rebuild to do."""
        if symbol:
            #print("add to arch eval: " + str(symbol.get_tree()))
            symbol.evalStatus = 0
            symbol.odooStatus = 0
            symbol.validationStatus = 0
            self.rebuild_arch_eval.add(symbol)

    def add_to_init_odoo(self, symbol):
        """ add a symbol to the list of odoo loading to do. if Force, the symbol will be added even if
        he is already validated"""
        if symbol:
            file = symbol.get_in_parents([SymType.FILE, SymType.PACKAGE, SymType.NAMESPACE])
            if not file:
                print("file not found, can't rebuild")
                return
            file.odooStatus = 0
            file.validationStatus = 0
            #print("add to init odoo: " + str(file.get_tree()))
            self.rebuild_odoo.add(file)

    def add_to_validations(self, symbol):
        """ add a symbol to the list of revalidation to do. if Force, the symbol will be added even if
        he is already validated"""
        if symbol:
            file = symbol.get_in_parents([SymType.FILE, SymType.PACKAGE, SymType.NAMESPACE])
            if not file:
                print("file not found, can't rebuild")
                return
            file.validationStatus = 0
            #print("add to validation: " + str(file.get_tree()))
            self.rebuild_validation.add(file)

    def _search_symbols_to_rebuild(self, tree):
        flat_tree = [item for l in tree for item in l]
        new_set_to_revalidate = RegisteredRefSet()
        for s in self.not_found_symbols:
            for p in s.not_found_paths:
                if flat_tree[:len(p)] == p[:len(flat_tree)]: #TODO wrong
                    new_set_to_revalidate.add(s)
                    #print("found one pending: " + str(s.get_tree()))
        return new_set_to_revalidate

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
