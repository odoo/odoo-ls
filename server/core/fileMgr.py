import ast
import os
import sys
from lsprotocol.types import (Diagnostic,Position, Range)
from urllib.request import quote

from ..constants import *

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
    def _getDefaultDict(path, version):
        return {
                "ast": None,
                "version": version,
                "uri": FileMgr.pathname2uri(path),
                "parsoTree": None, #parso tree is set only for opened documents
                "d_synt": [],
                "d_arch": [],
                "d_arch_eval": [],
                "d_odoo": [],
                "d_val": []
            }

    @staticmethod
    def getFileInfo(path, content=False, version=1, opened=False):
        f = FileMgr.files.get(path, None)
        if not f:
            f = FileMgr._getDefaultDict(path, version)
            f["ast"] = FileMgr._buildAST(path, f, content)
            FileMgr.files[path] = f
        elif content:
            if f["version"] < version:
                f["ast"] = FileMgr._buildAST(path, f, content)
                if opened:
                    f["parsoTree"] = FileMgr._buildParsoTree(path, f, content)
            elif opened and not f["parsoTree"]:
                f["parsoTree"] = FileMgr._buildParsoTree(path, f, content)
            f["version"] = version
        return f

    @staticmethod
    def removeParsoTree(path):
        f = FileMgr.files.get(path, None)
        if f:
            f["parsoTree"] = None

    @staticmethod
    def is_path_in_workspace(ls, path):
        for folder, _ in ls.workspace.folders.items():
            if path.startswith(folder):
                return True
        return False

    @staticmethod
    def clean_cache(ls, path):
        f = FileMgr.files.get(path, None)
        if f:
            f["ast"] = None
        FileMgr.removeParsoTree(path)

    @staticmethod
    def publish_diagnostics(ls, file):
        ls.publish_diagnostics(file["uri"], file["d_synt"] + file["d_arch"] + file["d_arch_eval"] + file["d_odoo"] + file["d_val"])

    @staticmethod
    def _buildParsoTree(path, fileInfo, content):
        from server.core.odoo import Odoo
        return Odoo.get().grammar.parse(content, error_recovery=True, cache = False)

    @staticmethod
    def _buildAST(path, fileInfo, content):
        try:
            if content:
                tree = ast.parse(content, path)
            else:
                with open(path, "rb") as f:
                    content = f.read()
                #tree = self.grammar.parse(content, error_recovery=False, path=self.filePath, cache = False)
                tree = ast.parse(content, path)
            fileInfo["d_synt"] = []
        except SyntaxError as e:
            diag = [Diagnostic(
                range = Range(
                    start=Position(line=e.lineno-1, character=e.offset),
                    end=Position(line=e.lineno-1, character=e.offset+1) if sys.version_info < (3, 10) else \
                        Position(line=e.end_lineno-1, character=e.end_offset)
                ),
                message = type(e).__name__ + ": " + e.msg,
                source = EXTENSION_NAME
            )]
            #if syntax is invalid, we have to drop all other diagnostics
            fileInfo["d_arch"] = []
            fileInfo["d_arch_eval"] = []
            fileInfo["d_odoo"] = []
            fileInfo["d_val"] = []
            fileInfo["d_synt"] = diag
            return False
        except ValueError as e:
            return False
        except PermissionError as e:
            return False
        return tree
