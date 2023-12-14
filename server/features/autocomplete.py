from ..constants import *
from ..core.odoo import Odoo
from ..core.symbol import Symbol
from ..features.hover import HoverFeature
from ..features.parso_utils import ParsoUtils
from lsprotocol.types import (CompletionItemKind, CompletionList, CompletionItemKind, CompletionItem,
                              CompletionItemLabelDetails, MarkupContent, MarkupKind)

class AutoCompleteFeature:

    @staticmethod
    def get_sort_text(symbol, cl, cl_to_complete):
        #return the text used for sorting the result for "symbol". cl is the class owner of symbol, and cl_to_completee the class
        # of the symbol to complete
        # ~ is used as last char of ascii table and } before last one
        base_dist = 0
        if cl_to_complete:
            base_dist = cl_to_complete.get_base_distance(cl.name)
        text = base_dist * "}" + (cl.name if cl else "") + symbol.name
        if symbol.name.startswith("_"):
            text = "~" + text
        if symbol.name.startswith("__"):
            text = "~" + text
        return text

    @staticmethod
    def build_symbol_completion_item(sym_to_complete, symbol):
        if isinstance(sym_to_complete, Symbol):
            cl_to_complete = sym_to_complete.get_in_parents([SymType.CLASS])
            cl = symbol.get_in_parents([SymType.CLASS])
            sym_type = symbol.follow_ref()[0]
            description = ""
            if sym_type.type == SymType.CLASS:
                description = sym_type.name
            elif sym_type == SymType.PRIMITIVE:
                description = sym_type.value
        else:
            cl_to_complete = None
            cl = symbol.get_in_parents([SymType.CLASS])
            model = cl.get_model()
            sym_type = symbol.follow_ref()[0]
            description = ""
            if sym_type.type == SymType.CLASS:
                description = sym_type.name
            elif sym_type == SymType.PRIMITIVE:
                description = sym_type.value
            if model:
                cl = model
                description = "(" + cl.name + ") " + description


        return CompletionItem(
            label=symbol.name,
            label_details = CompletionItemLabelDetails(
                detail="",
                description=description,
            ),
            sort_text = AutoCompleteFeature.get_sort_text(symbol, cl, cl_to_complete),
            documentation=HoverFeature.build_markdown_description(symbol, symbol, None, None),
            kind = AutoCompleteFeature._get_completion_item_kind(symbol),
        )

    @staticmethod
    def build_model_completion_list(models, module):
        """Return a CompletionList for the given models, module is needed to filter documentation"""
        module_acc = set()
        items = []
        for m in models:
            items.append(CompletionItem(
                label=m.name,
                documentation=m.get_documentation(module, module_acc),
                kind = CompletionItemKind.Interface if m.is_abstract(module, module_acc) else CompletionItemKind.Class,
            ))
        return CompletionList(
            is_incomplete=False,
            items=items
        )

    @staticmethod
    def autocomplete(path, content, line, char):
        parsoTree = Odoo.get().grammar.parse(content, error_recovery=True, cache = False)
        element = parsoTree.get_leaf_for_position((line, char-1), include_prefixes=True)
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
                module = file_symbol.get_module_sym()
                if not module:
                    return []
                models = Odoo.get().get_models(module, before)
                return AutoCompleteFeature.build_model_completion_list(models, module)
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
            module = file_symbol.get_module_sym()
            scope_symbol = file_symbol.get_scope_symbol(line)
            _, symbol_ancestors, _, context = ParsoUtils.evaluate_expr(expr, scope_symbol)
            if not symbol_ancestors:
                return []
            if isinstance(symbol_ancestors, list):
                symbol_ancestors = symbol_ancestors[0] #take the first override
            symbol_ancestors, _ = symbol_ancestors.follow_ref(context)
            symbol_ancestors = symbol_ancestors.get_model() or symbol_ancestors
            return CompletionList(
                is_incomplete=False,
                items=[AutoCompleteFeature.build_symbol_completion_item(symbol_ancestors, symbol)
                       for symbol in AutoCompleteFeature._get_symbols_from_obj(symbol_ancestors, module, context, -1)]
            )
        elif element and element.type == 'name':
            #TODO maybe not useful as vscode provide basic dictionnay autocompletion with seen names in the file
            expr = ParsoUtils.get_previous_leafs_expr(element)
            if not expr:
                file_symbol = Odoo.get().get_file_symbol(path)
                if not file_symbol:
                    return []
                module = file_symbol.get_module_sym()
                scope_symbol = file_symbol.get_scope_symbol(line)
                return CompletionList(
                    is_incomplete=False,
                    items=[AutoCompleteFeature.build_symbol_completion_item(scope_symbol, symbol)
                           for symbol in AutoCompleteFeature._get_symbols_from_obj(scope_symbol, module, {}, line, element.value)]
                )
        elif element and element.type == 'string':
            s = element.value
            if s[0] in ['"', "'"]:
                s = s[1:]
            if s[-1] in ['"', "'"]:
                s = s[:-1]
            before = s
            file_symbol = Odoo.get().get_file_symbol(path)
            module = file_symbol.get_module_sym()
            models = Odoo.get().get_models(module, before)
            if not models:
                return []
            expr = ParsoUtils.get_previous_leafs_expr(element)
            res = AutoCompleteFeature.build_model_completion_list(models, module)
            return res
        else:
            print("Automplete use case unknown")

    @staticmethod
    def _get_symbols_from_obj(obj, module, context, line=-1, starts_with = ""):
        """ For a symbol or model, get all sub symbols
        if line is not -1, seearch for local symbols before the line number
        """
        seen = set()
        if isinstance(obj, Symbol):
            for s in obj.all_symbols(line=line, include_inherits=True):
                if s.name.startswith(starts_with):
                    if s not in seen:
                        seen.add(s)
                        yield s
            if "comodel_name" in context and obj.is_inheriting_from((["odoo", "fields"], ["_Relational"])): #TODO better way to handle this hack
                model = Odoo.get().models.get(context["comodel_name"], None)
                if model:
                    models_syms = model.get_symbols(module)
                    for model_class in models_syms:
                        for s in model_class.all_symbols(line=-1, include_inherits=True):
                            if s not in seen:
                                seen.add(s)
                                yield s
        else:
            for a in obj.get_attributes(module):
                if a.name.startswith(starts_with):
                    if a not in seen:
                        seen.add(a)
                        yield a

    @staticmethod
    def _get_completion_item_kind(symbol):
        if symbol.type == SymType.CLASS:
            return CompletionItemKind.Class
        elif symbol.type == SymType.FUNCTION:
            return CompletionItemKind.Function
        elif symbol.type == SymType.VARIABLE:
            return CompletionItemKind.Variable
        else:
            return CompletionItemKind.Text
