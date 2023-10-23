import os
import parso
import pathlib
import re
import sys
import traceback
import threading
from collections import defaultdict
from ..odoo_language_server import OdooLanguageServer
from ..constants import *
from .symbol import RootSymbol
from .file_mgr import FileMgr
from .thread_condition import ReadWriteCondition
from ..references import RegisteredRefSet
from contextlib import contextmanager
from lsprotocol.types import (ConfigurationItem, WorkspaceConfigurationParams)
from pygls.server import LanguageServer, MessageType

#for debug
import time

import tracemalloc


class Odoo():

    instance = None
    import_odoo_addons = True #can be set to False for speed up tests

    def __init__(self):
        self.odooPath = ""

        self.version_major = 0
        self.version_minor = 0
        self.version_micro = 0
        self.stop_init = False

        self.refreshMode = "afterDelay"
        self.autoSaveDelay = 1000

        self.grammar = None

        self.models = {}
        self.modules = {}

        # symbols is the list of declared symbols and their related declaration, filtered by name
        self.symbols = RootSymbol("root", SymType.ROOT)
        self.builtins = RootSymbol("builtins", SymType.ROOT)

        self.rebuild_arch = RegisteredRefSet()
        self.rebuild_arch_eval = RegisteredRefSet()
        self.rebuild_odoo = RegisteredRefSet()
        self.rebuild_validation = RegisteredRefSet()

        self.not_found_symbols = RegisteredRefSet() # Set of symbols that still have unresolved dependencies (arch level only)

        self.write_lock = threading.Lock()
        self.thread_access_condition = ReadWriteCondition(10) #should match the number of threads

        self.stubs_dir = os.path.join(pathlib.Path(__file__).parent.parent.resolve(), "typeshed", "stubs")
        self.stdlib_dir = os.path.join(pathlib.Path(__file__).parent.parent.resolve(), "typeshed", "stdlib")

    @contextmanager
    def acquire_write(self, ls, timeout=-1):
        if OdooLanguageServer.access_mode.get() == "write":
            yield True
            return
        if self.write_lock.acquire(timeout=timeout):
            try:
                ls.send_notification('Odoo/loadingStatusUpdate', 'start')
                self.thread_access_condition.wait_empty()
                OdooLanguageServer.access_mode.set("write")
                yield Odoo.get() == self
                OdooLanguageServer.access_mode.set("none")
                ls.send_notification('Odoo/loadingStatusUpdate', 'stop')
            finally:
                self.write_lock.release()
        else:
            yield False

    @contextmanager
    def acquire_read(self, timeout=-1):
        if self.write_lock.acquire(timeout=timeout):
            try:
                self.thread_access_condition.acquire()
            finally:
                self.write_lock.release()
        else:
            yield False
            return
        OdooLanguageServer.access_mode.set("read")
        yield Odoo.get() == self # to be sure Odoo.instance is still bound to the same instance
        self.thread_access_condition.release()
        OdooLanguageServer.access_mode.set("none")

    @contextmanager
    def upgrade_to_write(self):
        if OdooLanguageServer.access_mode.get() == "write":
            yield
            return
        if OdooLanguageServer.access_mode.get() != "read":
            raise Exception("Can't upgrade to write from a non read lock")
        self.thread_access_condition.release()
        with self.acquire_write():
            yield
        with self.write_lock:
            self.thread_access_condition.acquire()
            OdooLanguageServer.access_mode.set("read")

    @staticmethod
    def get():
        return Odoo.instance

    @staticmethod
    def initialize(ls:LanguageServer = None):
        if not Odoo.instance:
            if not ls:
                print(f"Can't initialize Odoo Base : No odoo server provided. Please contact support.")
                return None
            ls.show_message_log("Building new Odoo knowledge database")

            # import cProfile
            # import pstats
            # profiler = cProfile.Profile()
            # profiler.enable()

            try:
                Odoo.instance = Odoo()
                odooConfig = ls.lsp.send_request("Odoo/getConfiguration").result()
                config = ls.get_configuration(WorkspaceConfigurationParams(items=[
                    ConfigurationItem(
                        scope_uri='window',
                        section="Odoo")
                ])).result()
                Odoo.instance.refreshMode = config[0]["autoRefresh"]
                Odoo.instance.autoSaveDelay = config[0]["autoRefreshDelay"]
                ls.file_change_event_queue.set_delay(Odoo.instance.autoSaveDelay)
                with Odoo.instance.acquire_write(ls):
                    Odoo.instance.symbols.paths = []
                    Odoo.instance.symbols.paths.append(Odoo.instance.stubs_dir)
                    Odoo.instance.symbols.paths.append(Odoo.instance.stdlib_dir)
                    for path in sys.path:
                        if os.path.isdir(path):
                            Odoo.instance.symbols.paths.append(path)
                    # add stubs for not installed packages
                    Odoo.instance.grammar = parso.load_grammar()
                    Odoo.instance.start_build_time = time.time()
                    Odoo.instance.odooPath = odooConfig.odooPath
                    if os.name == "nt":
                        Odoo.instance.odooPath = Odoo.instance.odooPath[0].capitalize() + Odoo.instance.odooPath[1:]
                    Odoo.instance.load_builtins(ls)
                    Odoo.instance.build_database(ls, odooConfig)
                    ls.show_message_log("End building database in " + str(time.time() - Odoo.instance.start_build_time) + " seconds")
            except Exception as e:
                ls.send_notification("Odoo/displayCrashNotification", {"crashInfo": traceback.format_exc()})
                ls.show_message_log(traceback.format_exc())
                print(traceback.format_exc())
                ls.show_message_log(f'Error ocurred: {e}', MessageType.Error)

            # profiler.disable()
            # stats = pstats.Stats(profiler)
            # stats.strip_dirs()
            # stats.dump_stats('/home/odoo/profiling_odoo.prof')

    def interrupt_initialization(self):
        self.stop_init = True

    def reset(self, ls):
        self.thread_access_condition.release()
        self.write_lock.release()
        with Odoo.instance.acquire_write(ls):
            Odoo.instance = None

    @staticmethod
    def reload_database(ls):
        if Odoo.get():
            ls.show_message_log("Interrupting initialization", MessageType.Log)
            Odoo.get().interrupt_initialization()
            ls.show_message_log("Reset existing database", MessageType.Log)
            Odoo.get().reset(ls)
        FileMgr.files = {}
        ls.show_message_log("Building new database", MessageType.Log)
        ls.show_message("Reloading Odoo database", MessageType.Info)
        ls.launch_thread(target=Odoo.initialize, args=(ls,))


    def get_symbol(self, fileTree, nameTree = []):
        return self.symbols.get_symbol(fileTree, nameTree)

    def load_builtins(self, ls):
        from .python_arch_builder import PythonArchBuilder
        builtins_path = os.path.join(pathlib.Path(__file__).parent.parent.resolve(), "typeshed", "stdlib", "builtins.pyi")
        parser = PythonArchBuilder(ls, self.builtins, builtins_path)
        parser.load_arch()
        self.process_rebuilds(ls)

    def build_database(self, ls, used_config):
        if not self.build_base(ls, used_config):
            return False
        self.build_modules(ls)

    def build_base(self, ls, used_config):
        from .python_arch_builder import PythonArchBuilder
        releasePath = os.path.join(self.odooPath, "odoo", "release.py")
        if os.path.exists(releasePath):
            with open(releasePath, "r") as f:
                lines = f.readlines()
                for line in lines:
                    if line.startswith("version_info ="):
                        reg = r"version_info = \((['\"]?(\D+~)?\d+['\"]?, \d+, \d+, \w+, \d+, \D+)\)"
                        match = re.match(reg, line)
                        if match:
                            res = match.group(1).split(", ")
                            self.version_major = int(res[0].split("saas~")[-1].replace("'", "").replace('"', ''))
                            self.version_minor = int(res[1])
                            self.version_micro = int(res[2])
                        else:
                            self.version_major, self.version_minor, self.version_micro = 14, 0, 0
                            ls.show_message("Unable to detect the Odoo version. Running the tool for the version 14", MessageType.Error)
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
            addonsSymbol.paths += used_config.addons
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
        from .python_arch_builder import PythonArchBuilder
        from .python_arch_eval import PythonArchEval
        from ..features.validation.python_validator import PythonValidator
        if DEBUG_REBUILD:
            ls.show_message_log("starting rebuild process")
            arch_rebuilt = []
            eval_rebuilt = []
            odoo_rebuilt = []
            validation_rebuilt = []
        already_arch_rebuilt = set()
        while not self.stop_init:
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
                path = sym.get_paths()[0]
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
                from .python_odoo_builder import PythonOdooBuilder
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
        from .python_arch_builder import PythonArchBuilder
        addonsSymbol = self.symbols.get_symbol(["odoo", "addons"])
        addonsPaths = self.symbols.get_symbol(["odoo", "addons"]).paths
        for path in addonsPaths:
            dirs = os.listdir(path)
            for dir in dirs:
                if os.path.isdir(os.path.join(path, dir)):
                    PythonArchBuilder(ls, addonsSymbol, os.path.join(path, dir)).load_arch(require_module=True)
            if self.stop_init:
                break
        if self.stop_init:
            return
        #needed?
        self.process_rebuilds(ls)

        try:
            import psutil
            ls.show_message_log("ram usage : " + str(psutil.Process(os.getpid()).memory_info().rss / 1024 ** 2) + " Mo")
        except Exception:
            ls.show_message_log("ram usage: unknown (please install psutil to get ram usage)")
            pass
        ls.show_message_log(str(len(Odoo.get().modules)) + " modules found")

    def get_file_symbol(self, path):
        addonSymbol = self.symbols.get_symbol(["odoo", "addons"])
        if not addonSymbol:
            return []
        for dir_path in [self.instance.odooPath] + addonSymbol.paths:
            if path.startswith(dir_path):
                tree = path.replace(".py", "")[len(dir_path)+1:].replace("\\", "/").split("/")
                if tree:
                    if dir_path != self.instance.odooPath:
                        tree = addonSymbol.get_tree()[0] + tree
                    else:
                        if tree[0] == "addons":
                            tree = ["odoo"] + tree
                    if tree[-1] in ["__init__", "__manifest__"]:
                        tree.pop()
                return self.symbols.get_symbol(tree)
        return []

    def _unload_path(self, ls, path, clean_cache=False):
        """unload the symbol with 'path'. If clean_cache==True, remove the matching cache from FileMgr.
        Return the parent symbol of the unloaded symbol"""
        file_symbol = self.get_file_symbol(path)
        parent = None
        if file_symbol:
            parent = file_symbol.parent
            if clean_cache:
                FileMgr.delete_path(ls, path)
                s = list(file_symbol.moduleSymbols.values())
                for sym in s:
                    FileMgr.delete_path(ls, sym.get_paths()[0])
                    s.extend(sym.moduleSymbols.values())
            file_symbol.unload(file_symbol)
        return parent

    def _build_new_symbol(self, ls, path, parent):
        """ build a new symbol for the file at 'path' and return the new symbol tree"""
        from .python_arch_builder import PythonArchBuilder
        if path.endswith("__init__.py") or path.endswith("__init__.pyi") or path.endswith("__manifest__.py"):
            path = os.sep.join(path.split(os.sep)[:-1])
        pp = PythonArchBuilder(ls, parent, path)
        new_symbol = pp.load_arch()
        new_symbol_tree = new_symbol.get_tree()
        return new_symbol_tree

    def file_change(self, ls, path, text, version):
        #snapshot1 = tracemalloc.take_snapshot()
        if path.endswith(".py"):
            ls.show_message_log("File change event: " + path + " version " + str(version))
            with Odoo.get().acquire_write(ls):
                file_info = FileMgr.get_file_info(path, text, version, opened=True)
                file_info.publish_diagnostics(ls)
                if file_info.version != version: #if the update didn't work
                    return
                #1 unload
                parent = self._unload_path(ls, path, False)
                if not parent:
                    return
                #build new
                new_symbol_tree = self._build_new_symbol(ls, path, parent)
                #rebuild validations
                if new_symbol_tree:
                    self._search_symbols_to_rebuild(new_symbol_tree)
                # self.process_rebuilds(ls) #No more process, it is done at end of queue at (EventQueue.py/process)
        #snapshot2 = tracemalloc.take_snapshot()
        #top_stats = snapshot2.compare_to(snapshot1, 'lineno')

    def file_delete(self, ls, path):
        with Odoo.get().acquire_write(ls):
            self._unload_path(ls, path, True)

    def file_create(self, ls, path):
        with Odoo.get().acquire_write(ls):
            new_parent = self.get_file_symbol(os.sep.join(path.split(os.sep)[:-1]))
            self._build_new_symbol(ls, path, new_parent)
            new_tree = new_parent.get_tree()
            new_tree[1].append(path.split(os.sep)[-1].replace(".py", ""))
            rebuilt_needed = self._search_symbols_to_rebuild(new_tree)
            if rebuilt_needed or new_parent.get_tree() == (["odoo", "addons"], []):
                #if there is something that is trying to import the new file, build it.
                #Else, don't add it to the architecture to not add useless symbols (and overrides)
                new_tree = self._build_new_symbol(ls, path, new_parent)

    def add_to_rebuilds(self, symbols):
        """add a dictionnary of symbols to the rebuild list. The dict must have the format
        {BuildStep: Iterator[symbols]}"""
        for s in symbols.get(BuildSteps.ARCH, []):
            self.add_to_arch_rebuild(s)
        for s in symbols.get(BuildSteps.ARCH_EVAL, []):
            self.add_to_arch_eval(s)
        for s in symbols.get(BuildSteps.ODOO, []):
            self.add_to_init_odoo(s)
        for s in symbols.get(BuildSteps.VALIDATION, []):
            self.add_to_validations(s)

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
        """ Consider the given 'tree' path as updated (or new) and move all symbols that were searching for it
        from the not_found_symbols list to the rebuild list. Return True is something should be rebuilt"""
        flat_tree = [item for l in tree for item in l]
        new_dict_to_revalidate = defaultdict(lambda: RegisteredRefSet())
        found_symbols = RegisteredRefSet()
        for s in self.not_found_symbols:
            for index in range(len(s.not_found_paths)):
                step, tree = s.not_found_paths[index]
                if flat_tree[:len(tree)] == tree[:len(flat_tree)]:
                    new_dict_to_revalidate[step].add(s)
                    del s.not_found_paths[index]
            if not s.not_found_paths:
                found_symbols.add(s)
        self.not_found_symbols -= found_symbols
        need_rebuild = bool(new_dict_to_revalidate)
        if need_rebuild:
            self.add_to_rebuilds(new_dict_to_revalidate)
        return need_rebuild

    def get_models(self, module = None, start_name = ""):
        res = []
        module_acc = set()
        for name, model in self.models.items():
            if name.startswith(start_name):
                if module:
                    if model.get_main_symbols(module, module_acc):
                        res += [model]
                else:
                    res += [model]
        return res
