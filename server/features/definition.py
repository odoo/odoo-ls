import os

from server.features.parsoUtils import ParsoUtils
from server.constants import *
from server.core.fileMgr import FileMgr
from lsprotocol.types import (Location, Range, Position)

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
        if not isinstance(symbol, list):
            symbol = [symbol]
        res = []
        for s in symbol:
            for path in s.paths: #to be sure, but it should always have a length of 1, who would want to see the definition of odoo.addons?
                range = Range(
                    start=Position(line=s.startLine, character=0),
                    end=Position(line=s.endLine, character=1)
                )
                res.append(Location(
                    uri=FileMgr.pathname2uri(path),
                    range=range
                ))
        return res