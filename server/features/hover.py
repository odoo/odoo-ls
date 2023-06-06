from server.features.parsoUtils import ParsoUtils
from server.core.model import Model
from server.core.odoo import Odoo
from lsprotocol.types import (Hover, MarkupContent, MarkupKind, Range, Position)

class HoverFeature:

    @staticmethod
    def getSymbol(fileSymbol, content, line, character):
        "return the Symbol at the given position in a file"
        range = None
        scope_symbol = fileSymbol.get_scope_symbol(line)
        parsoTree = Odoo.get().grammar.parse(content, error_recovery=True, cache = False)
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
                return ""
            evaluation = evaluation.get_main_symbols(module)
            if len(evaluation) == 1:
                evaluation = evaluation[0]
            else:
                return None
        return evaluation, range, context

    @staticmethod
    def get_Hover(symbol, range, context):
        if not symbol:
            return Hover(None)
        type_ref = symbol.follow_ref(context)
        infered_type = ""
        if type_ref[1]:
            infered_type = ": " + type_ref[0].name
        type = str(symbol.type).lower()
        class_doc = type_ref[0].doc and type_ref[0].doc.eval.value if type_ref[1] else ""
        value = "(" + type + ") **" + symbol.name + "**" + infered_type
        if symbol.doc:
            value += "  \n-  \n**" + symbol.name + "**:" + symbol.doc.eval.value
        if infered_type:
            value += "  \n-  \n**" + infered_type[2:] + "**: " + class_doc
        content = MarkupContent(
            kind=MarkupKind.Markdown,
            value=value
        )
        return Hover(
            contents=content,
            range=range
        )