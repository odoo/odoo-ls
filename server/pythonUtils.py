import traceback
from lsprotocol.types import MessageType
from functools import wraps
from .constants import *
from pathlib import Path
import ast

class PythonUtils():

    case_preserving_fs = Path(__file__.upper()).is_file()

    @staticmethod
    def _exists_cs(p: Path) -> bool:
        """Check if path exists, enforce case sensitivity.

        Arguments:
        p: Path to check
        Returns:
        Boolean indicating if the path exists or not
        """
        if not p.exists():
            return False

        if PythonUtils.case_preserving_fs:
            while p.parent != p:
                # If string representation of path is not in parent directory, return False
                if str(p) not in map(str, p.parent.iterdir()):
                    return False
                p = p.parent

        return True

    @staticmethod
    def is_file_cs(path:str):
        p = Path(path)
        return p.is_file() and PythonUtils._exists_cs(p)

    @staticmethod
    def is_dir_cs(path:str):
        p = Path(path)
        return p.is_dir() and PythonUtils._exists_cs(p)

    # @staticmethod
    # def inferTypeParso(expr):
    #     return None

    # @staticmethod
    # def evaluateTypeParso(node_list, scope_symbol):
    #     """return the symbol of the type of the expr. if the expr represent a function call, the function symbol is returned.
    #     If you want to infer the symbol corresponding to an expr when evaluation, use inferTypeParso"""
    #     obj = None #symbol or model
    #     node_iter = 0
    #     while node_iter != len(node_list):
    #         node = node_list[node_iter]
    #         if not obj:
    #             if node.type == "name" and node.value == "self":
    #                 obj = scope_symbol.get_in_parents([SymType.CLASS]) #should be able to take it in func param, no?
    #             else:
    #                 infer = scope_symbol.inferName(node.value, node.line)
    #                 if not infer:
    #                     return None
    #                 obj, _ = infer.follow_ref()
    #                 if obj.type == SymType.VARIABLE:
    #                     return None
    #         else:
    #             if node.type == "trailer":
    #                 if node.children[0].type == "operator" and node.children[0].value == ".":
    #                     if obj.isModel() and node.children[1].value == "env" \
    #                         and node_iter != len(node_list) and node_list[node_iter+1].type == "trailer" \
    #                         and node_list[node_iter+1].children[0].type == "operator" \
    #                         and node_list[node_iter+1].children[0].value == "[" \
    #                         and node_list[node_iter+1].children[1].type == "string":
    #                         node_iter += 1
    #                         obj = Odoo.get().models[node_list[node_iter].children[1].value.replace("'", "").replace('"', '')]
    #                     else:
    #                         module = scope_symbol.get_module()
    #                         obj = obj.get_class_symbol(node.children[1].value, module)
    #                     if not obj:
    #                         return None
    #         node_iter += 1
    #     return obj

    # @staticmethod
    # def get_complete_expr(content, line, character):
    #     #TODO go to previous line and skip comments
    #     full_expr = ""
    #     cl = line
    #     cc = character
    #     canContinue = True
    #     space = False
    #     special_closures = []
    #     while canContinue:
    #         char = content[cl][cc]
    #         if char in ['"', "'", '(', '{', '[']:
    #             if (special_closures[-1] == ")" and char == "(") or \
    #                     (special_closures[-1] == "}" and char == "{") or \
    #                     (special_closures[-1] == "]" and char == "[") or \
    #                     (special_closures[-1] == '"' and char == '"') or \
    #                     (special_closures[-1] == "'" and char == "'"):
    #                 special_closures.pop()
    #             elif len(special_closures) == 0:
    #                 space = False
    #                 canContinue = False
    #         elif char == ' ' and not space:
    #             space = True
    #         elif char == '.':
    #             space = False
    #         elif char in [')', '}', ']', '"', "'"]:
    #             special_closures.append(char)
    #         elif char in [',', '+', '/', '*', '-', '%', '>', '<', '=', '!', '&', '|', '^', '~', ':']:
    #             canContinue = False
    #         else:
    #             if space:
    #                 canContinue = False
    #         if canContinue:
    #             full_expr = char + full_expr
    #         cc -= 1
    #         if cc < 0:
    #             cl -= 1
    #             if cl < 0:
    #                 canContinue = False
    #             else:
    #                 cc = len(content[cl]) - 1
    #     print(full_expr)
    #     if special_closures:
    #         return ''
    #     return full_expr

    # @staticmethod
    # def get_parent_symbol(file_symbol, line, expr):
    #     current_symbol = None
    #     for e in expr:
    #         if e[0] == 'self':
    #             current_symbol = file_symbol.get_class_scope_symbol(line + 1)
    #         elif current_symbol:
    #             pass
    #         else:
    #             #try to find in localAliases
    #             pass
    #     return current_symbol

    # @staticmethod
    # def get_atom_expr(parsoTree, line, char):
    #     """for a parsoTree, return three data for a cursor at 'line', 'char':
    #     last_atomic_expr: the full parso tree matching the atomic expression holding the cursor
    #     list_expr: a list of parsotree containing each Attribute (self.env[].search will be split in 4)
    #         but, if the cursor is on env, the result will be [self, env[]] only. The last element is always 'current'
    #     current: the parso tree of the smallest (without child) node containing the cursor"""
    #     current = parsoTree
    #     last_atomic_expr = None
    #     list_expr = []
    #     while hasattr(current, "children") and current.type != "trailer":
    #         if current.type == 'atom_expr':
    #             last_atomic_expr = current
    #             list_expr = []
    #         found_cursor = False
    #         for c in current.children:
    #             if c.type == "trailer":
    #                 if c.children[0].type == "operator" and c.children[0].value == ".":
    #                     if found_cursor:
    #                         break
    #             #we don't check the start line and char because comments like this one are not in nodes
    #             if (c.end_pos[0] > line or c.end_pos[0] == line and c.end_pos[1] >= char) and not found_cursor:
    #                 if (c.start_pos[0] > line or c.start_pos[0] == line and c.start_pos[1] > char):
    #                     return (None, [], None)
    #                 current = c
    #                 found_cursor = True
    #             list_expr.append(c)
    #     if not last_atomic_expr:
    #         list_expr = [current]
    #     print(list_expr)
    #     return (last_atomic_expr, list_expr, current)

    @staticmethod
    def unpack_assign(node_targets, node_annotation, node_values, acc = {}):
        """ Unpack assignement to extract variables and values.
            This method will return a dictionnary that hold each variables and the set value (still in ast node)
            example: variable = variable2 = "test" (2 targets, 1 value)
            ast.Assign => {"variable": ast.Node("test"), "variable2": ast.Node("test")}
         """
        if isinstance(node_targets, ast.Attribute) or isinstance(node_targets, ast.Subscript):
            return acc
        if isinstance(node_targets, ast.Name):
            acc[node_targets] = node_values
            return acc
        if isinstance(node_targets, ast.Tuple) and not isinstance(node_values, ast.Tuple):
            #we can't unpack (a,b) = c as we can't unpack c here
            return acc
        for target in node_targets:
            if isinstance(target, ast.Name):
                acc[target] = node_values
            elif isinstance(target, ast.Tuple) and isinstance(node_values, ast.Tuple):
                if len(target.elts) != len(node_values.elts):
                    print("ERROR: unable to unpack assignement")
                    return acc
                else:
                    #TODO handle a,b = b,a
                    for nt, nv in zip(target.elts, node_values.elts):
                        PythonUtils.unpack_assign(nt, nv, acc)
            elif isinstance(target, ast.Tuple):
                for elt in target.elts:
                    #We only want local variables
                    if isinstance(elt, ast.Name):
                        pass #TODO to infer this, we should be able to follow right values (func for example) and unsplit it
            else:
                pass
                # print("ERROR: unpack_assign not implemented for " + str(node_targets) + " and " + str(node_values))
        return acc

def send_error_on_traceback(func):
    @wraps(func)
    def wrapper_func(*args, **kwargs):
        from server.OdooLanguageServer import odoo_server
        try:
            return func(*args, **kwargs)
        except Exception:
            odoo_server.show_message_log(traceback.format_exc(), MessageType.Error)
            odoo_server.send_notification("Odoo/displayCrashNotification", {"crashInfo": traceback.format_exc()})
    return wrapper_func