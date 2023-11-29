from ..constants import *
from ..core.odoo import Odoo
from ..core.model import Model
from ..core.symbol import Symbol
from lsprotocol.types import (Range, Position)

class ParsoUtils:

    @staticmethod
    def get_symbols(fileSymbol, parsoTree,line, character):
        "return the Symbols at the given position in a file, the range of the selected symbol and the context"
        range = None
        scope_symbol = fileSymbol.get_scope_symbol(line)
        element = parsoTree.get_leaf_for_position((line, character), include_prefixes=True)
        range = Range(
            start=Position(line=element.start_pos[0]-1, character=element.start_pos[1]),
            end=Position(line=element.end_pos[0]-1, character=element.end_pos[1])
        )
        expr = ParsoUtils.get_previous_leafs_expr(element)
        expr.append(element)
        symbol, context = ParsoUtils.evaluate_expr(expr, scope_symbol)
        if isinstance(symbol, Model):
            module = fileSymbol.get_module_sym()
            if not module:
                return "Can't evaluate the current module. Are you in a valid Odoo module?", None, None
            symbol = symbol.get_main_symbols(module)
            if len(symbol) == 1:
                symbol = symbol[0]
            else:
                return "Can't find the definition: 'Multiple models with the same name exists.'", None, None
        elif isinstance(symbol, str):
            module = fileSymbol.get_module_sym()
            if module:
                model = Odoo.get().models.get(symbol, None)
                if model:
                    symbol = model.get_main_symbols(module)
                    if len(symbol) == 1:
                        return symbol[0], range, context
                    else:
                        return "Can't find the definition: 'Multiple models with the same name exists.'", None, None
        elif isinstance(symbol, list):
            return symbol, range, context
        return symbol, range, context

    @staticmethod
    def get_previous_leafs_expr(leaf):
        #given a leaf, return a list of leafs that are forming a whole expression (atomic)
        #ex:
        # self.env["test"].search([]) => given "search" leaf, will return [self, env, [, "test", ], ., search]]]
        # => given "test", will return ["test"]
        # => given "env", will return [self, ., env]

        def inverse_bracket(b):
            if b == '[':
                return ']'
            elif b == '(':
                return ')'
            return ''

        leafs = []
        previous = leaf.get_previous_leaf()
        brackets = []
        while previous:
            if previous.type == 'operator':
                if previous.value in [']', ')']:
                    brackets.insert(0, previous.value)
                elif previous.value in ['[', '(']:
                    if len(brackets) > 0:
                        if brackets[0] != inverse_bracket(previous.value):
                            print("Invalid expression")
                            return []
                        brackets.pop(0)
                    else:
                        break
                elif previous.value == '.':
                    pass
                elif len(brackets) == 0:
                    break
            if previous.type == 'newline':
                break
            if previous.type == "keyword":
                break
            leafs.insert(0, previous)
            previous = previous.get_previous_leaf()
        return leafs

    @staticmethod
    def evaluate_expr(node_list, scope_symbol):
        """return the symbol of the type of the expr.
        Can return a list of symbols to represent all overrides. In this case the first symbol of the list is the symbol overriding others.
        Ex:
        self.env["ir.model"].search([]).id will return the symbol if "id" in the "ir.model" model
        """
        obj = None #symbol or model
        node_iter = 0
        module = scope_symbol.get_module_sym()
        context = {
            "args": None,
            "parent": None,
            "module": module,
        }

        def prepare_evaluation():
            nonlocal obj, context
            # sym = self.symbols[name]
            # if sym.symbols["__get__"]:
            #     sym = sym.symbols["__get__"].eval and sym.symbols["__get__"].eval.get_symbol() or sym
            # if isinstance(obj, Symbol) and "comodel_name" in context and \
            # obj.is_inheriting_from((["odoo", "fields"], ["_Relational"])): #TODO better way to handle this hack
            #     model = Odoo.get().models.get(context["comodel_name"], None)
            #     if model:
            #         main_sym = model.get_main_symbols(module)
            #         if main_sym and len(main_sym) == 1:
            #             objs = main_sym[0].get_member_symbol(next_element.value, all=True)
            if isinstance(obj, list):
                obj = obj[0] #take the most relevant symbol if multiple overrides exist
            if not isinstance(obj, Model):
                obj, _ = obj.follow_ref(context)

        while node_iter != len(node_list):
            node = node_list[node_iter]
            if not obj:
                obj = scope_symbol.infer_name(node.value, node.line +1) #+1 to be able to take itself if needed
                if not obj:
                    if node.type == "string":
                        return node.value[1:-1], context
                    return None, context
            else:
                prepare_evaluation()
                if node.type == "operator":
                    if node.value == "." and len(node_list) > node_iter+1:
                        if obj.type == SymType.VARIABLE:
                            return None, context
                        node_iter += 1
                        next_element = node_list[node_iter]
                        context["parent"] = obj
                        obj = obj.get_member_symbol(next_element.value, all=True)
                        if not obj:
                            return None, context
                        #evaluate __get__ if it exists
                        # for index in range(len(obj)):
                        #     o = obj[index]
                        #     if o.type != SymType.VARIABLE:
                        #         continue
                        #     get_func = o.get_symbol([], "__get__")
                        #     if not get_func or not get_func.eval or not get_func.eval.get_symbol(context):
                        #         continue
                        #     obj[index] = get_func.eval.get_symbol(context)
                    elif node.value == "[" and len(node_list) > node_iter+1:
                        if obj.type == SymType.VARIABLE:
                            return None, context
                        inner_part = []
                        node_iter += 1
                        while node_iter < len(node_list) and (node_list[node_iter].value != "]" or node_list[node_iter].parent != node.parent):
                            inner_part.append(node_list[node_iter])
                            node_iter += 1
                        if node_iter >= len(node_list) or node_list[node_iter].value != "]" or node_list[node_iter].parent != node.parent:
                            return None, context
                        content = ParsoUtils.evaluate_expr(inner_part, scope_symbol)[0]
                        get_item_sym = obj.get_member_symbol("__getitem__", module)
                        if not get_item_sym:
                            return None, context
                        get_item_sym = get_item_sym.follow_ref(context)[0]
                        context["parent"] = obj
                        context["args"] = content
                        obj = get_item_sym.eval.get_symbol(context)
                        context["args"] = None
                    elif node.value == "(" and len(node_list) > node_iter+1:
                        if obj.type != SymType.FUNCTION:
                            return None, context
                        if not obj.eval:
                            return None, context
                        inner_part = []
                        node_iter += 1
                        while node_iter < len(node_list) and (node_list[node_iter].value != ")" or node_list[node_iter].parent != node.parent):
                            inner_part.append(node_list[node_iter])
                            node_iter += 1
                        if node_iter >= len(node_list) or node_list[node_iter].value != ")" or node_list[node_iter].parent != node.parent:
                            return None, context
                        args = []
                        if inner_part:
                            i = 0
                            arg = []
                            while i < len(inner_part):
                                if inner_part[i].value == "," and inner_part[1].parent and inner_part[1].parent.parent == node:
                                    args += ParsoUtils.evaluate_expr(arg, scope_symbol)[0]
                                    arg = []
                                else:
                                    arg.append(inner_part[i])
                                i+=1
                        # context["parent"] = obj #we don't want the function to be parent of itself
                        context["args"] = args
                        obj = obj.eval.get_symbol(context)
                        context["args"] = None
            node_iter += 1
        return obj, context