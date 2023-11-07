import ast
import os
import sys
import urllib
from lsprotocol.types import (Diagnostic,Position, Range)
from urllib.request import quote

from ..constants import *

class FileInfo:

    def __init__(self, path, version):
        self.ast = None
        self.version = version
        self.uri = FileMgr.pathname2uri(path)
        self.parso_tree = None #only available for opened documents
        self.need_push = False
        self.diagnostics = {
            BuildSteps.SYNTAX: [],
            BuildSteps.ARCH: [],
            BuildSteps.ARCH_EVAL: [],
            BuildSteps.ODOO: [],
            BuildSteps.VALIDATION: []
        }

    def build_ast(self, path, content=False):
        try:
            if content:
                self.ast = ast.parse(content, path)
            else:
                with open(path, "rb") as f:
                    content = f.read()
                #tree = self.grammar.parse(content, error_recovery=False, path=self.filePath, cache = False)
                self.ast = ast.parse(content, path)
            self.replace_diagnostics(BuildSteps.SYNTAX, [])
        except SyntaxError as e:
            diag = [Diagnostic(
                range = Range(
                    start=Position(line=e.lineno-1, character=e.offset-1),
                    end=Position(line=e.lineno-1, character=e.offset-1) if sys.version_info < (3, 10) else \
                        Position(line=e.end_lineno-1, character=e.end_offset-1 if e.end_offset > 0 else 0)
                ),
                message = type(e).__name__ + ": " + e.msg,
                source = EXTENSION_NAME
            )]
            #if syntax is invalid, we have to drop all other diagnostics
            self.replace_diagnostics(BuildSteps.ARCH, [])
            self.replace_diagnostics(BuildSteps.ARCH_EVAL, [])
            self.replace_diagnostics(BuildSteps.ODOO, [])
            self.replace_diagnostics(BuildSteps.VALIDATION, [])
            self.replace_diagnostics(BuildSteps.SYNTAX, diag)
            return False
        except ValueError as e:
            diag = [Diagnostic(
                range = Range(
                    start=Position(line=0, character=0),
                    end=Position(line=0, character=0)
                ),
                message = "File contains null characters",
                source = EXTENSION_NAME
            )]
            #if syntax is invalid, we have to drop all other diagnostics
            self.replace_diagnostics(BuildSteps.ARCH, [])
            self.replace_diagnostics(BuildSteps.ARCH_EVAL, [])
            self.replace_diagnostics(BuildSteps.ODOO, [])
            self.replace_diagnostics(BuildSteps.VALIDATION, [])
            self.replace_diagnostics(BuildSteps.SYNTAX, diag)
            return False
        except PermissionError as e:

            diag = [Diagnostic(
                range = Range(
                    start=Position(line=0, character=0),
                    end=Position(line=0, character=0)
                ),
                message = "PermissionError: Odoo extension does not have permission to read this file",
                source = EXTENSION_NAME
            )]
            #if syntax is invalid, we have to drop all other diagnostics
            self.replace_diagnostics(BuildSteps.ARCH, [])
            self.replace_diagnostics(BuildSteps.ARCH_EVAL, [])
            self.replace_diagnostics(BuildSteps.ODOO, [])
            self.replace_diagnostics(BuildSteps.VALIDATION, [])
            self.replace_diagnostics(BuildSteps.SYNTAX, diag)
            return False
        return True

    def build_parso_tree(self, path, content):
        from .odoo import Odoo
        self.parso_tree = Odoo.get().grammar.parse(content, error_recovery=True, cache = False)

    def replace_diagnostics(self, step, diagnostics):
        old = self.diagnostics[step]
        self.diagnostics[step] = diagnostics
        if old != diagnostics:
            self.need_push = True

    def publish_diagnostics(self, ls):
        if self.need_push:
            self.need_push = False
            ls.publish_diagnostics(self.uri, self.diagnostics[BuildSteps.SYNTAX]
                                + self.diagnostics[BuildSteps.ARCH]
                                + self.diagnostics[BuildSteps.ARCH_EVAL]
                                + self.diagnostics[BuildSteps.ODOO]
                                + self.diagnostics[BuildSteps.VALIDATION])
            return True
        return False

class FileMgr():

    files = {}

    @staticmethod
    def pathname2uri(str):
        if os.name == 'nt':
            #TODO fix hack
            str = str[0].lower() + str[1:]
        str = str.replace("\\", "/")
        str = quote(str)
        f = "file://"
        if os.name == "nt":
            f += "/"
        str = f + str
        return str

    @staticmethod
    def uri2pathname(uri):
        path = urllib.parse.urlparse(urllib.parse.unquote(uri)).path
        path = urllib.request.url2pathname(path)
        #TODO find better than this small hack for windows (get disk letter in capital)
        if os.name == "nt":
            path = path[0].capitalize() + path[1:]
        return path

    @staticmethod
    def get_file_info(path, content=False, version=1, opened=False):
        """ a version of -100 and empty content force a reload from disk"""
        if os.name == "nt":
            path = path[0].capitalize() + path[1:]
        f = FileMgr.files.get(path, None)
        if version == -100 and not content:
            with open(path, "rb") as file:
                content = file.read()
        if not f:
            f = FileInfo(path, version)
            f.build_ast(path, content)
            FileMgr.files[path] = f
            f.version = version if version != -100 else 1
        elif content:
            if f.version < version or version == -100:
                valid = f.build_ast(path, content)
                if valid:
                    f.version = version if version != -100 else f.version
                    if opened:
                        f.build_parso_tree(path, content)
                else:
                    f.parso_tree = None
            elif opened and not f.parso_tree:
                f.build_parso_tree(path, content)
        return f

    @staticmethod
    def get_file(path):
        return FileMgr.files.get(path, None)

    @staticmethod
    def is_path_in_workspace(ls, path):
        for folder, _ in ls.workspace.folders.items():
            folder_path = FileMgr.uri2pathname(folder)
            if path.startswith(folder_path):
                return True
        return False

    @staticmethod
    def delete_path(ls, path):
        to_del = FileMgr.files.pop(path, None)
        if to_del:
            to_del.need_push = True
            to_del.replace_diagnostics(BuildSteps.ARCH, [])
            to_del.replace_diagnostics(BuildSteps.ARCH_EVAL, [])
            to_del.replace_diagnostics(BuildSteps.ODOO, [])
            to_del.replace_diagnostics(BuildSteps.VALIDATION, [])
            to_del.replace_diagnostics(BuildSteps.SYNTAX, [])
            to_del.publish_diagnostics(ls)

    @staticmethod
    def delete_info(path):
        fileInfo = FileMgr.files.get(path, None)
        if fileInfo:
            fileInfo.ast = None
            fileInfo.parso_tree = None

    @staticmethod
    def delete_parso(path):
        fileInfo = FileMgr.files.get(path, None)
        if fileInfo:
            fileInfo.parso_tree = None

    @staticmethod
    def reset_diagnostics(ls):
        for file in FileMgr.files.values():
            for d in file.diagnostics.values():
                if d:
                    ls.publish_diagnostics(file.uri, [])
                    break
