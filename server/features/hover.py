import os

from server.features.parsoUtils import ParsoUtils
from server.core.model import Model
from server.core.odoo import Odoo
from server.constants import *
from server.core.fileMgr import FileMgr
from lsprotocol.types import (Hover, MarkupContent, MarkupKind, Range, Position)

from server.core import fileMgr

class HoverFeature:

    @staticmethod
    def getSymbol(fileSymbol, parsoTree,line, character):
        "return the Symbol at the given position in a file"
        range = None
        scope_symbol = fileSymbol.get_scope_symbol(line)
        element = parsoTree.get_leaf_for_position((line, character), include_prefixes=True)
        range = Range(
            start=Position(line=element.start_pos[0]-1, character=element.start_pos[1]),
            end=Position(line=element.end_pos[0]-1, character=element.end_pos[1])
        )
        expr = ParsoUtils.get_previous_leafs_expr(element)
        expr.append(element)
        evaluation, context = ParsoUtils.evaluateType(expr, scope_symbol)
        if isinstance(evaluation, Model):
            module = fileSymbol.get_module()
            if not module:
                return "Can't evaluate the current module. Are you in a valid Odoo module?", None, None
            evaluation = evaluation.get_main_symbols(module)
            if len(evaluation) == 1:
                evaluation = evaluation[0]
            else:
                return "Can't find the definition: 'Multiple models with the same name exists.'", None, None
        elif isinstance(evaluation, str):
            module = fileSymbol.get_module()
            if module:
                model = Odoo.get().models.get(evaluation, None)
                if model:
                    evaluation = model.get_main_symbols(module)
                    if len(evaluation) == 1:
                        return evaluation[0], range, context
                    else:
                        return "Can't find the definition: 'Multiple models with the same name exists.'", None, None
        return evaluation, range, context

    @staticmethod
    def get_Hover(symbol, range, context):

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

        if not symbol:
            return Hover(None)
        if isinstance(symbol, str):
            return Hover(symbol)
        type_ref = symbol.follow_ref(context)
        infered_type = ""
        if type_ref[0] != symbol:
            infered_type = type_ref[0].name
        type = str(symbol.type).lower()
        if symbol.type == SymType.VARIABLE and not type_ref[1]:
            type = str(type_ref[0].type).lower()
            if type_ref[0].type == SymType.FILE:
                type = "module"
        class_doc = type_ref[0].doc and type_ref[0].doc.eval.value if type_ref[1] else ""
        #BLOCK 1: (type) **name** -> infered_type
        value = build_block_1(symbol, type, infered_type)
        #SEPARATOR
        value += "  \n***  \n"
        #BLOCK 2: useful links:
        if infered_type:
            path = FileMgr.pathname2uri(type_ref[0].paths[0])
            if type_ref[0].type == SymType.PACKAGE:
                path = os.path.join(path, "__init__.py")
            value += "useful links: " + "[" + type_ref[0].name + "](" + path + "#" + str(type_ref[0].startLine) + ")" + "  \n"
            #SEPARATOR
            value += "  \n***  \n"
        #BLOCK 3: doc
        if symbol.doc and symbol.doc.eval:
            value += "  \n-  \n" + symbol.doc.eval.value
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