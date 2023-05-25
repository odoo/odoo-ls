from server.pythonUtils import PythonUtils
from server.core.odoo import Odoo

class HoverFeature:

    @staticmethod
    def getSymbol(fileSymbol, content, line, character):
        "return the Symbol at the given position in a file"
        scope_symbol = fileSymbol.get_scope_symbol(line)
        parsoTree = Odoo.get().grammar.parse(content, error_recovery=False, path=fileSymbol.paths[0], cache = False)
        atom_expr, parent_expr, expr = PythonUtils.get_atom_expr(parsoTree, line, character)
        symbol = PythonUtils.evaluateTypeParso(parent_expr, scope_symbol)
        return symbol