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
            return None
        if isinstance(symbol, str):
            return Hover(symbol)
        if isinstance(symbol, list):
            symbol = symbol[0]
        return HoverFeature._build_hover(symbol, range, context)

    @staticmethod
    def build_markdown_description(symbol, context):

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
                elif symbol.name != infered_type:
                    value += ": " + infered_type
            value += "  \n```"
            return value

        type_ref = symbol.follow_ref(context)
        infered_type = "Any"
        if type_ref[0] != symbol:
            infered_type = type_ref[0].name
        type = str(symbol.type).lower()
        if symbol.type == SymType.FUNCTION:
            if symbol.is_property:
                type = "property"
            else:
                type = "method"
        #class_doc = type_ref[0].doc and type_ref[0].doc.value if type_ref[1] else ""
        #BLOCK 1: (type) **name** -> infered_type
        value = build_block_1(symbol, type, infered_type)
        #BLOCK 2: useful links:
        if infered_type not in ["Any", "constant"]:
            paths = type_ref[0].get_paths()
            if paths:
                path = FileMgr.pathname2uri(paths[0])
                if type_ref[0].type == SymType.PACKAGE:
                    path = os.path.join(path, "__init__.py")
                value += "  \n***  \n"
                value += "useful links: " + "[" + type_ref[0].name + "](" + path + "#" + str(type_ref[0].start_pos[0]) + ")" + "  \n"
        #BLOCK 3: doc
        if symbol.doc:
            value += "  \n***  \n" + symbol.doc.value
        #if infered_type:
        #    value += "  \n-  \n**" + infered_type[2:] + "** : " + class_doc
        if symbol.name == "tomate" and symbol.type == SymType.VARIABLE: #easter egg (private joke)
            value = "Please rename your variable. Tomate is not a good name for a variable. You won't know what it means in 2 weeks (or even earlier)"
        return MarkupContent(
            kind=MarkupKind.Markdown,
            value=value
        )

    @staticmethod
    def _build_hover(symbol, range, context):
        return Hover(
            contents=HoverFeature.build_markdown_description(symbol, context),
            range=range
        )