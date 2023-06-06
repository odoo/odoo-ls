from server.constants import *
from server.core.odoo import Odoo
from server.core.model import Model

class ParsoUtils:

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
            leafs.insert(0, previous)
            previous = previous.get_previous_leaf()
        return leafs

    @staticmethod
    def evaluateType(node_list, scope_symbol):
        """return the symbol of the type of the expr. if the expr represent a function call, the function symbol is returned.
        If you want to infer the symbol corresponding to an expr when evaluation, use inferTypeParso"""
        obj = None #symbol or model
        node_iter = 0
        context = {}
        while node_iter != len(node_list):
            node = node_list[node_iter]
            if not obj:
                if node.type == "name" and node.value == "self":
                    obj = scope_symbol.get_in_parents([SymType.CLASS]) #should be able to take it in func param, no?
                else:
                    infer = scope_symbol.inferName(node.value, node.line)
                    if not infer:
                        return None, context
                    obj, _ = infer.follow_ref()
                    if obj.type == SymType.VARIABLE:
                        return None, context
            else:
                if node.type == "operator" and node.value == "." and len(node_list) > node_iter+1:
                    next_element = node_list[node_iter+1]
                    #if obj.isModel() and next_element.value == "env" \
                    # TODO change to get_item
                    if next_element.value == "env" \
                        and len(node_list) > node_iter + 4 \
                        and node_list[node_iter+2].type == "operator" \
                        and node_list[node_iter+2].value == "[" \
                        and node_list[node_iter+3].type == "string":
                        obj = Odoo.get().models[node_list[node_iter+3].value.replace("'", "").replace('"', '')]
                        node_iter += 4
                    else:
                        module = scope_symbol.get_module()
                        if not isinstance(obj, Model):
                            obj = obj.follow_ref(context)[0]
                        obj = obj.get_class_symbol(next_element.value, module)
                    if not obj:
                        return None, context
            node_iter += 1
        return obj, context