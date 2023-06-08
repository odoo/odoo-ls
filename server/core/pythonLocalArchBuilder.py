import ast
import weakref
from server.constants import *
from server.core.symbol import Symbol
from server.core.evaluation import Evaluation
from server.pythonUtils import PythonUtils


class PythonLocalArchBuilder(ast.NodeVisitor):

    def __init__(self, symbol):
        self.symbol = symbol

    def enhance(self):
        if not self.symbol.enhanced:
            print("Enhancing symbol: " + self.symbol.name + " at " + self.symbol.paths[0])
            self.symbol.enhanced = True
            ast_node = self.symbol.ast_node()
            if not ast_node:
                return
            self.visit(ast_node)
    
    def visit_Assign(self, node):
        assigns = PythonUtils.unpack_assign(node.targets, node.value, {})
        for variable_name, value in assigns.items():
            if self.symStack[-1].type in [SymType.CLASS, SymType.FILE, SymType.PACKAGE]:
                variable = Symbol(variable_name, SymType.VARIABLE, self.filePath)
                variable.startLine = node.lineno
                variable.endLine = node.end_lineno
                variable.ast_node = weakref.ref(node)
                variable.eval = Evaluation().evalAST(value, self.symbol)
                self.symbol.add_symbol(variable)