import os

from server.features.parsoUtils import ParsoUtils
from server.constants import *
from server.core.fileMgr import FileMgr
from lsprotocol.types import (Location)

class DefinitionFeature:

    @staticmethod
    def get_location(fileSymbol, parsoTree,line, character):

        def build_block_1(symbol, type, infered_type):
            value =  "```python  \n"
            value += "(" + type + ") "
            if symbol.type == SymType.FUNCTION:
                value += "def "
            value += symbol.name
            if symbol.type == SymType.FUNCTION and symbol.ast_node:
                value += "(  \n" + ",  \n".join(arg.arg for arg in symbol.ast_node.args.args) + "  \n)"
            if infered_type and type != "module":
                if symbol.type == SymType.FUNCTION:
                    value += " -> " + infered_type
                else:
                    value += " : " + infered_type
            value += "  \n```"
            return value

        symbol, range, context = ParsoUtils.getSymbols(fileSymbol, parsoTree, line, character)

        if not symbol:
            return []
        if isinstance(symbol, str):
            return []
        return [Location(
            uri=FileMgr.pathname2uri(path),
            range=r
        ) for path, r in symbol.paths]