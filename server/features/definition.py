import os

from ..features.parso_utils import ParsoUtils
from ..constants import *
from ..core.file_mgr import FileMgr
from lsprotocol.types import (Location, Range, Position)

class DefinitionFeature:

    @staticmethod
    def get_location(fileSymbol, parsoTree,line, character):

        symbol, range, context = ParsoUtils.get_symbols(fileSymbol, parsoTree, line, character)

        if not symbol:
            return []
        if isinstance(symbol, str):
            return []
        if not isinstance(symbol, list):
            symbol = [symbol]
        res = []
        for s in symbol:
            for path in s.get_paths(): #to be sure, but it should always have a length of 1, who would want to see the definition of odoo.addons?
                range = Range(
                    start=Position(line=s.start_pos[0]-1, character=s.start_pos[1]),
                    end=Position(line=s.end_pos[0]-1, character=s.end_pos[1])
                )
                res.append(Location(
                    uri=FileMgr.pathname2uri(path),
                    range=range
                ))
        return res