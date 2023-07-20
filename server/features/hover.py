import os

from server.features.parsoUtils import ParsoUtils
from server.constants import *
from server.core.fileMgr import FileMgr
from lsprotocol.types import (Hover, MarkupContent, MarkupKind)

class HoverFeature:

    @staticmethod
    def get_Hover(fileSymbol, parsoTree, line, character):

        symbol, range, context = ParsoUtils.getSymbols(fileSymbol, parsoTree, line, character)

        if not symbol:
            return Hover(None)
        if isinstance(symbol, str):
            return Hover(symbol)
        if isinstance(symbol, list):
            symbol = symbol[0]
        return HoverFeature._build_hover(symbol, range, context)

    @staticmethod
    def _build_hover(symbol, range, context):


        def build_block_1(symbol, type, infered_type):
            value =  "```python  \n"
            value += "(" + type + ") "
            if symbol.type == SymType.FUNCTION and not symbol.is_property:
                value += "def "
            value += symbol.name
            if symbol.type == SymType.FUNCTION and not symbol.is_property and symbol.ast_node:
                value += "(  \n" + ",  \n".join(arg.arg for arg in symbol.ast_node.args.args) + "  \n)"
            if infered_type and type != "module":
                if symbol.type == SymType.FUNCTION and not symbol.is_property:
                    value += " -> " + infered_type
                else:
                    value += " : " + infered_type
            value += "  \n```"
            return value

        type_ref = symbol.follow_ref(context)
        infered_type = "Any"
        if type_ref[0] != symbol:
            infered_type = type_ref[0].name
        type = str(symbol.type).lower()
        if symbol.type == SymType.VARIABLE and not type_ref[1]:
            type = str(type_ref[0].type).lower()
            if type_ref[0].type == SymType.FILE:
                type = "module"
        if symbol.type == SymType.FUNCTION:
            if symbol.is_property:
                type = "property"
            else:
                type = "method"
        #class_doc = type_ref[0].doc and type_ref[0].doc.value if type_ref[1] else ""
        #BLOCK 1: (type) **name** -> infered_type
        value = build_block_1(symbol, type, infered_type)
        #SEPARATOR
        value += "  \n***  \n"
        #BLOCK 2: useful links:
        if infered_type not in ["Any"]:
            path = FileMgr.pathname2uri(type_ref[0].paths[0])
            if type_ref[0].type == SymType.PACKAGE:
                path = os.path.join(path, "__init__.py")
            value += "useful links: " + "[" + type_ref[0].name + "](" + path + "#" + str(type_ref[0].startLine) + ")" + "  \n"
            #SEPARATOR
            value += "  \n***  \n"
        #BLOCK 3: doc
        if symbol.doc:
            value += "  \n-  \n" + symbol.doc.value
        #if infered_type:
        #    value += "  \n-  \n**" + infered_type[2:] + "** : " + class_doc
        content = MarkupContent(
            kind=MarkupKind.Markdown,
            value=value
        )
        return Hover(
            contents=content,
            range=range
        )