from server.constants import *
from server.core.odoo import Odoo
from server.core.symbol import Symbol
from server.features.parsoUtils import ParsoUtils
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
                module = file_symbol.get_module()
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
            # containers = []
            # previous = element.get_previous_leaf()
            # first_leaf = element.search_ancestor("error_node").get_first_leaf()
            # while previous:
            #     containers.insert(0, previous)
            #     if previous == first_leaf:
            #         break
            #     previous = previous.get_previous_leaf()
            # print(containers)
            expr = ParsoUtils.get_previous_leafs_expr(element)
            file_symbol = Odoo.get().get_file_symbol(path)
            if not file_symbol:
                return []
            module = file_symbol.get_module()
            scope_symbol = file_symbol.get_scope_symbol(line)
            symbol_ancestors, context = ParsoUtils.evaluateType(expr, scope_symbol)
            if not symbol_ancestors:
                return []
            if isinstance(symbol_ancestors, list):
                symbol_ancestors = symbol_ancestors[0] #take the first override
            symbol_ancestors = symbol_ancestors.get_model() or symbol_ancestors
            return CompletionList(
                is_incomplete=False,
                items=[CompletionItem(
                    label=symbol.name,
                    documentation=str(symbol.type).lower(),
                    kind = AutoCompleteFeature._getCompletionItemKind(symbol),
                ) for symbol in AutoCompleteFeature._get_symbols_from_obj(symbol_ancestors, module, context, -1)]
            )
        elif element and element.type == 'name':
            #TODO maybe not useful as vscode provide basic dictionnay autocompletion with seen names in the file
            expr = ParsoUtils.get_previous_leafs_expr(element)
            if not expr:
                file_symbol = Odoo.get().get_file_symbol(path)
                if not file_symbol:
                    return []
                module = file_symbol.get_module()
                scope_symbol = file_symbol.get_scope_symbol(line)
                return CompletionList(
                    is_incomplete=False,
                    items=[CompletionItem(
                        label=symbol.name,
                        #documentation=symbol.doc,
                        kind = AutoCompleteFeature._getCompletionItemKind(symbol),
                    ) for symbol in AutoCompleteFeature._get_symbols_from_obj(scope_symbol, module, {}, line, element.value)]
                )
        elif element and element.type == 'string':
            s = element.value
            if s[0] in ['"', "'"]:
                s = s[1:]
            if s[-1] in ['"', "'"]:
                s = s[:-1]
            before = s
            file_symbol = Odoo.get().get_file_symbol(path)
            module = file_symbol.get_module()
            models = Odoo.get().get_models(module, before)
            if not models:
                return []
            expr = ParsoUtils.get_previous_leafs_expr(element)
            return CompletionList(
                is_incomplete=False,
                items=[CompletionItem(
                    label=m.name,
                    documentation=m.get_documentation(module),
                    kind = CompletionItemKind.Interface if m.is_abstract(module) else CompletionItemKind.Class,
                ) for m in models]
            )
        else:
            print("Automplete use case unknown")

    @staticmethod
    def _get_symbols_from_obj(obj, module, context, line=-1, starts_with = ""):
        """ For a symbol or model, get all sub symbols
        if line is not -1, seearch for local symbols before the line number
        """
        seen = set()
        if isinstance(obj, Symbol):
            def_obj = obj.follow_ref(context)[0]
            for s in def_obj.all_symbols(line=line, include_inherits=True):
                if s.name.startswith(starts_with) and (not s.name.startswith("__") or starts_with.startswith("__")):
                    if s not in seen:
                        seen.add(s)
                        print(s.name)
                        yield s
            if "comodel_name" in context and def_obj.is_inheriting_from((["odoo", "fields"], ["_Relational"])): #TODO better way to handle this hack
                model = Odoo.get().models.get(context["comodel_name"], None)
                if model:
                    models_syms = model.get_symbols(module)
                    for model_class in models_syms:
                        for s in model_class.all_symbols(line=-1, include_inherits=True):
                            if s not in seen:
                                seen.add(s)
                                print(s.name)
                                yield s
        else:
            for a in obj.get_attributes(module):
                if a.name.startswith(starts_with) and (not a.name.startswith("__") or starts_with.startswith("__")):
                    if a not in seen:
                        seen.add(a)
                        print(a.name)
                        yield a

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
