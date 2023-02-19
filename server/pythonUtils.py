from .constants import *
from .odoo import *
from .symbol import *
from .model import *

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
                return f.id
            elif isinstance(f, ast.Attribute):
                return PythonUtils.evaluateTypeAST(f, symbol)
        elif isinstance(node, ast.Attribute):
            v = PythonUtils.evaluateTypeAST(node.value, symbol)
            if v and node.attr in v.symbols:
                return v.symbols[node.attr]
        elif isinstance(node, ast.Name):
            sym = symbol
            while sym and sym.inferencer.inferName(node.id, node.lineno) and sym.type != "file":
                sym = sym.parent
            infered = sym.inferencer.inferName(node.id, node.lineno)
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
    def evaluateTypeParso(parent_list, scope_symbol):
        """return the symbol of the type of the expr. if the expr represent a function call, the function symbol is returned.
        If you want to infer the symbol corresponding to an expr when evaluation, use inferTypeParso"""
        symbol = None
        if parent_list and parent_list[-1].type == "operator" and parent_list[-1].value == ".":
            parent = PythonUtils.evaluateTypeParso(parent_list[0:-2], parent_list[-2], scope_symbol)
            if not parent:
                return None
            if expr.value in parent.symbols:
                return parent.symbols[expr.value]
            if parent.bases: #if is an odoo class (at least inherit model)
                if expr.value == "env":
                    pass
        else:
            if expr.type == "name" and expr.value == "self":
                return scope_symbol.get_in_parents("class")
            scope_sym = scope_symbol
            while scope_sym and expr.value not in scope_sym.localAliases and scope_sym.type != "file":
                scope_sym = scope_sym.parent
            if expr.value in scope_sym.localAliases:
                return scope_sym.localAliases[expr.value]
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
        current = parsoTree
        last_atomic_expr = None
        list_expr = [[]]
        while hasattr(current, "children"):
            if current.type == 'atom_expr':
                last_atomic_expr = current
                list_expr = [[]]
            for c in current.children:
                if (c.start_pos[0] < line or c.start_pos[0] == line and c.start_pos[1] <= char) and \
                    (c.end_pos[0] > line or c.end_pos[0] == line and c.end_pos[1] >= char):
                    list_expr[-1].append(c)
                    current = c
                    break
                if c.type == "operator" and c.value == ".":
                    list_expr.append([])
                else:
                    list_expr[-1].append(c)
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