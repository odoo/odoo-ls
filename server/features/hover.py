from server.features.parsoUtils import ParsoUtils
from server.core.model import Model
from server.core.odoo import Odoo

class HoverFeature:

    @staticmethod
    def getSymbol(fileSymbol, content, line, character):
        "return the Symbol at the given position in a file"
        scope_symbol = fileSymbol.get_scope_symbol(line)
        parsoTree = Odoo.get().grammar.parse(content, error_recovery=True, cache = False)
        element = parsoTree.get_leaf_for_position((line+1, character), include_prefixes=True)
        expr = ParsoUtils.get_previous_leafs_expr(element)
        expr.append(element)
        evaluation = ParsoUtils.evaluateType(expr, scope_symbol)
        if isinstance(evaluation, Model):
            module_symbol = fileSymbol.get_module()
            if not module_symbol:
                return []
            module = Odoo.get().modules.get(module_symbol.name, None)
            evaluation = evaluation.get_main_symbols(module)
            if len(evaluation) == 1:
                evaluation = evaluation[0]
            else:
                return None
        return evaluation