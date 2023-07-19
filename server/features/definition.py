import os

from server.features.parsoUtils import ParsoUtils
from server.constants import *
from server.core.fileMgr import FileMgr
from lsprotocol.types import (Location, Range, Position)

class DefinitionFeature:

    @staticmethod
    def get_location(fileSymbol, parsoTree,line, character):

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
                    start=Position(line=s.startLine-1, character=0),
                    end=Position(line=s.endLine-1, character=1)
                )
                res.append(Location(
                    uri=FileMgr.pathname2uri(path),
                    range=range
                ))
        return res