import ast
from ..constants import *
import os
from .odoo import Odoo
from .file_mgr import FileMgr
from .python_arch_builder import PythonArchBuilder
from ..references import RegisteredRef
from .symbol import ConcreteSymbol
from lsprotocol.types import (Diagnostic, DiagnosticTag, Position,
                             Range, DiagnosticSeverity)

class ModuleSymbol(ConcreteSymbol):

    rootPath = ""
    loaded = False

    module_name = ""
    dir_name = ""
    depends = ["base"]
    data = []

    def __init__(self, ls, dir_path):
        self.valid = True
        self.dir_name = os.path.split(dir_path)[1]
        super().__init__(self.dir_name, SymType.PACKAGE, dir_path)
        self.rootPath = dir_path
        manifestPath = os.path.join(dir_path, "__manifest__.py")
        if not os.path.exists(manifestPath):
            self.valid = False
            return
        f = FileMgr.get_file_info(manifestPath)
        diagnostics = []
        diagnostics += self.load_manifest(f)
        if self.dir_name in Odoo.get().modules:
            pass
        Odoo.get().modules[self.dir_name] = RegisteredRef(self)
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
        f = FileMgr.get_file_info(os.path.join(self.rootPath, "__manifest__.py"))
        f.replace_diagnostics(BuildSteps.ARCH_EVAL, diagnostics) #ARCH_EVAL to use another level
        f.publish_diagnostics(ls)
        return loaded

    def load_manifest(self, fileInfo):
        """ Load manifest to identify the module characteristics
        Returns list of diagnostics to publish in manifest file """
        ast_body = fileInfo.ast.body
        if len(ast_body) != 1 or (len(ast_body) > 1 and (not isinstance(ast_body[0], ast.Expr) or not ast_body[0].value or not isinstance(ast_body[0].value, ast.Dict))):
            return [Diagnostic(
                range = Range(
                    start=Position(line=0, character=0),
                    end=Position(line=0, character=1)
                ),
                message = "A manifest should only contains one dictionnary",
                source = EXTENSION_NAME,
                severity= DiagnosticSeverity.Error,
            )]
        dic = ast_body[0].value
        self.module_name = ""
        self.data = []
        self.depends = []
        diags = []
        for key, value in zip(dic.keys, dic.values):
            if not isinstance(key, ast.Constant):
                diags.append(self._create_diag(key, "Manifest keys should be strings", DiagnosticSeverity.Error))
            if key.value == "name":
                if not isinstance(value, ast.Constant):
                    diags.append(self._create_diag(key, "Name value should be a string", DiagnosticSeverity.Error))
                self.module_name = value.value
            elif key.value == "depends":
                if not isinstance(value, ast.List):
                    diags.append(self._create_diag(key, "depends value should be a list of string", DiagnosticSeverity.Error))
                    continue
                l = []
                for el in value.elts:
                    if not isinstance(el, ast.Constant):
                        diags.append(self._create_diag(key, "dependency should be expressed with a string", DiagnosticSeverity.Error))
                        continue
                    l.append(el.value)
                self.depends = l
            elif key.value == "data":
                if not isinstance(value, ast.List):
                    diags.append(self._create_diag(key, "data value should be a list of string", DiagnosticSeverity.Error))
                    continue
                l = []
                for el in value.elts:
                    if not isinstance(el, ast.Constant):
                        diags.append(self._create_diag(key, "data file should be expressed with a string", DiagnosticSeverity.Error))
                        continue
                    l.append(el.value)
                self.data = l
            elif key.value == "active":
                diags.append(Diagnostic(
                    range = Range(
                        start=Position(line=key.lineno-1, character=key.col_offset-1),
                        end=Position(line=key.end_lineno-1, character=key.end_col_offset-1)
                    ),
                    message = "'active' is deprecated and has been replaced by 'auto_install'",
                    source = EXTENSION_NAME,
                    tags=[DiagnosticTag.Deprecated],
                    severity= DiagnosticSeverity.Error,
                ))
            else:
                if key.value not in ["version", "description", "author", "website", "license",
                                     "category", "demo", "auto_install", "external_dependencies",
                                     "application", "assets", "installable", "maintainer",
                                     "pre_init_hook", "post_init_hook", "uninstall_hook", "sequence",
                                     "summary", "icon", "url"]:
                    pass #diags.append(self._create_diag(key, "Unkown key value", DiagnosticSeverity.Error))
        if self.dir_name != 'base':
            self.depends.append("base")
        return diags

    def _create_diag(self, node, message, severity=DiagnosticSeverity.Error):
        return Diagnostic(
            range = Range(
                start=Position(line=node.lineno-1, character=node.col_offset+1),
                end=Position(line=node.end_lineno-1, character=node.end_col_offset-1)
            ),
            message = message,
            source = EXTENSION_NAME,
            severity= severity,
        )

    def load_depends(self, ls):
        """ ensure that all modules indicates in the module dependencies are well loaded.
        Returns list of diagnostics to publish in manifest file """
        diagnostics = []
        loaded = []
        for depend in self.depends:
            if depend not in Odoo.get().modules:
                from .import_resolver import find_module
                dep_module = find_module(ls, depend)
                if not dep_module:
                    Odoo.get().not_found_symbols.add(self)
                    self.not_found_paths.append((BuildSteps.ARCH, ["odoo", "addons", depend]))
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
            else:
                self.add_dependency(Odoo.get().modules[depend].ref, BuildSteps.ARCH, BuildSteps.ARCH)
        return diagnostics, loaded

    def _load_data(self):
        return []

    def _load_arch(self, ls, path):
        if os.path.exists(os.path.join(path, "tests")):
            tests_parser = PythonArchBuilder(ls, self, os.path.join(path, "tests"))
            tests_parser.load_arch()
        return []

    def is_in_deps(self, dir_name, module_acc:set=None):
        """Return True if dir_name is in the dependencies of the module.
        A module_acc can be given to speedup the search in dependencies by holding previous results"""
        if self.dir_name == dir_name or dir_name in self.depends:
            return True
        for dep in self.depends:
            if module_acc and dep in module_acc:
                return True
            dep_module = Odoo.get().modules.get(dep, None)
            if not dep_module:
                continue
            is_in = dep_module.ref.is_in_deps(dir_name)
            if is_in:
                if module_acc is not None:
                    module_acc.add(dep)
                return True
        return False
