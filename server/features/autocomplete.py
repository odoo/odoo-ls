from server.constants import *
from server.core.odoo import Odoo
from lsprotocol.types import (CompletionItemKind, CompletionList, CompletionItemKind, CompletionItem)

class AutoCompleteFeature:

    @staticmethod
    def autocomplete(path, content, line, char):
        from ..pythonUtils import PythonUtils
        parsoTree = Odoo.get().grammar.parse(content, error_recovery=True, cache = False)
        element = parsoTree.get_leaf_for_position((line+1, char), include_prefixes=True)
        #Test assignement
        assigns = []
        i = 1
        while element and hasattr(element, "children") and len(element.children) > i and element.children[i].type == "operator" and \
            element.children[i].value == "=" and element.children[i-1].type == "name":
                assigns.append(element.children[i-1].value)
                i += 2
        i -= 2
        if assigns:
            if "_inherit" in assigns:
                assign_part = element.children[i+1]
                if char < assign_part.start_pos[1] or char > assign_part.end_pos[1]:
                    return []
                before = assign_part.get_code()[:char-assign_part.start_pos[1]+1].strip()
                #valid before statements (>< is the cursor): 
                # _inherit = "><something"
                # _inherit = ["><something", "><something"]
                if not before or before[-1] not in ["'", '"']:
                    return []
                before = before[1:]
                file_symbol = Odoo.get().get_file_symbol(path)
                module_symbol = file_symbol.get_module()
                if not module_symbol:
                    return []
                module = Odoo.get().modules.get(module_symbol.name, None)
                if not module:
                    return []
                models = Odoo.get().get_models(module, before)
                return CompletionList(
                    is_incomplete=False,
                    items=[CompletionItem(
                        label=m.name,
                        documentation=m.get_documentation(module),
                        kind = CompletionItemKind.Interface if m.is_abstract(module) else CompletionItemKind.Class,
                    ) for m in models]
                )
        #Try to complete expression
        if element and element.type == 'operator' and element.value == ".":
            containers = []
            previous = element.get_previous_leaf()
            first_leaf = element.search_ancestor("error_node").get_first_leaf()
            while previous:
                containers.insert(0, previous)
                if previous == first_leaf:
                    break
                previous = previous.get_previous_leaf()
            file_symbol = Odoo.get().get_file_symbol(path)
            scope_symbol = file_symbol.get_scope_symbol(line)
            symbol_ancestors = PythonUtils.evaluateTypeParso(containers, scope_symbol)
            if symbol_ancestors:
                return CompletionList(
                    is_incomplete=False,
                    items=[CompletionItem(
                        label=symbol.name,
                        #documentation=symbol.doc,
                        kind = AutoCompleteFeature._getCompletionItemKind(symbol),
                    ) for symbol in symbol_ancestors.all_symbols(local=False)]
                )
            return []

    @staticmethod
    def _getCompletionItemKind(symbol):
        if symbol.type == SymType.CLASS:
            return CompletionItemKind.Class
        elif symbol.type == SymType.FUNCTION:
            return CompletionItemKind.Function
        elif symbol.type == SymType.VARIABLE:
            return CompletionItemKind.Variable
        else:
            return CompletionItemKind.Text