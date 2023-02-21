from .constants import *
from .odoo import *
from .symbol import *
from .model import *
from urllib.request import quote
import os

def pathname2uri(str):
    if os.name == 'nt':
        #TODO fix hack
        str = str[0].lower() + str[1:]
    str = str.replace("\\", "/")
    str = quote(str)
    str = "file:///" + str
    return str

class PythonUtils():

        #TODO evaluateType should not be based on ast?
    @staticmethod
    def evaluateTypeAST(node, symbol):
        """try to return the symbol corresponding to the expression, evaluated in the context of 'symbol' (a function, class or file)"""
        if isinstance(node, ast.Constant):
            return Symbol("constant", "primitive", "")
        elif isinstance(node, ast.Dict):
            return Symbol("dict", "primitive", "")
        elif isinstance(node, ast.Call):
            f = node.func
            if isinstance(f, ast.Name):
                infered = Inferencer.inferNameInScope(f.id, f.lineno, symbol)
                if infered:
                    return infered.symbol
            elif isinstance(f, ast.Attribute):
                return PythonUtils.evaluateTypeAST(f, symbol)
        elif isinstance(node, ast.Attribute):
            v = PythonUtils.evaluateTypeAST(node.value, symbol)
            if v and node.attr in v.symbols:
                return v.symbols[node.attr]
        elif isinstance(node, ast.Name):
            infered = Inferencer.inferNameInScope(node.id, node.lineno, symbol)
            if infered:
                return infered.symbol
        return None
    
    @staticmethod
    def inferTypeParso(expr):
        return None
    
    @staticmethod
    def _parso_split_node_dot(exprs):
        results = [exprs[0]]
        for c in results[1:]:
            if c.type == "operator" and c.value == ".":
                results.append([])
            else:
                results[-1].append(c)
        return results

    @staticmethod
    def evaluateTypeParso(node_list, scope_symbol):
        """return the symbol of the type of the expr. if the expr represent a function call, the function symbol is returned.
        If you want to infer the symbol corresponding to an expr when evaluation, use inferTypeParso"""
        symbol = None
        node_iter = 0
        while node_iter != len(node_list):
            node = node_list[node_iter]
            if not symbol:
                if node.type == "name" and node.value == "self":
                    symbol = scope_symbol.get_in_parents("class") #should be able to take it in func param, no?
                else:
                    infer = scope_symbol.inferencer.inferName(node.value, node.line)
                    if not infer:
                        return None
                    symbol = infer.symbol
            else:
                if node.type == "trailer":
                    if node.children[0].type == "operator" and node.children[0].value == ".":
                        if symbol.modelName and node.children[1].value == "env" \
                            and node_iter != len(node_list) and node_list[node_iter+1].type == "trailer" \
                            and node_list[node_iter+1].children[0].type == "operator" \
                            and node_list[node_iter+1].children[0].value == "[" \
                            and node_list[node_iter+1].children[1].type == "string":
                            node_iter += 1
                            model = Odoo.get().models[node_list[node_iter].children[1].value.replace("'", "").replace('"', '')]
                            if model:
                                symbol = model.get_main_symbol()
                        else:
                            symbol = symbol.get_class_symbol(node.children[1].value)
                        if not symbol:
                            return None
            node_iter += 1
        return symbol

    @staticmethod
    def get_complete_expr(content, line, character):
        #TODO go to previous line and skip comments
        full_expr = ""
        cl = line
        cc = character
        canContinue = True
        space = False
        special_closures = []
        while canContinue:
            char = content[cl][cc]
            if char in ['"', "'", '(', '{', '[']:
                if (special_closures[-1] == ")" and char == "(") or \
                        (special_closures[-1] == "}" and char == "{") or \
                        (special_closures[-1] == "]" and char == "[") or \
                        (special_closures[-1] == '"' and char == '"') or \
                        (special_closures[-1] == "'" and char == "'"):
                    special_closures.pop()
                elif len(special_closures) == 0:
                    space = False
                    canContinue = False
            elif char == ' ' and not space:
                space = True
            elif char == '.':
                space = False
            elif char in [')', '}', ']', '"', "'"]:
                special_closures.append(char)
            elif char in [',', '+', '/', '*', '-', '%', '>', '<', '=', '!', '&', '|', '^', '~', ':']:
                canContinue = False
            else:
                if space:
                    canContinue = False
            if canContinue:
                full_expr = char + full_expr
            cc -= 1
            if cc < 0:
                cl -= 1
                if cl < 0:
                    canContinue = False
                else:
                    cc = len(content[cl]) - 1
        print(full_expr)
        if special_closures:
            return ''
        return full_expr

    @staticmethod
    def get_parent_symbol(file_symbol, line, expr):
        current_symbol = None
        for e in expr:
            if e[0] == 'self':
                current_symbol = file_symbol.get_class_scope_symbol(line + 1)
            elif current_symbol:
                pass
            else:
                #try to find in localAliases
                pass
        return current_symbol

    @staticmethod
    def get_atom_expr(parsoTree, line, char):
        """for a parsoTree, return three data for a cursor at 'line', 'char':
        last_atomic_expr: the full parso tree matching the atomic expression holding the cursor
        list_expr: a list of parsotree containing each Attribute (self.env[].search will be split in 4)
            but, if the cursor is on env, the result will be [self, env[]] only. The last element is always 'current'
        current: the parso tree of the smallest (without child) node containing the cursor"""
        current = parsoTree
        last_atomic_expr = None
        list_expr = []
        while hasattr(current, "children") and current.type != "trailer":
            if current.type == 'atom_expr':
                last_atomic_expr = current
                list_expr = []
            found_cursor = False
            for c in current.children:
                if c.type == "trailer":
                    if c.children[0].type == "operator" and c.children[0].value == ".":
                        if found_cursor:
                            break
                if (c.start_pos[0] < line or c.start_pos[0] == line and c.start_pos[1] <= char) and \
                    (c.end_pos[0] > line or c.end_pos[0] == line and c.end_pos[1] >= char):
                    current = c
                    found_cursor = True
                list_expr.append(c)
        if not last_atomic_expr:
            list_expr = [current]
        print(list_expr)
        return (last_atomic_expr, list_expr, current)

    @staticmethod
    def getSymbol(fileSymbol, line, character):
        "return the Symbol at the given position in a file"
        with open(fileSymbol.paths[0], "rb") as f:
            content = f.read()
        scope_symbol = fileSymbol.get_scope_symbol(line)
        parsoTree = Odoo.get().grammar.parse(content, error_recovery=False, path=fileSymbol.paths[0], cache = False)
        atom_expr, parent_expr, expr = PythonUtils.get_atom_expr(parsoTree, line, character)
        symbol = PythonUtils.evaluateTypeParso(parent_expr, scope_symbol)
        return symbol