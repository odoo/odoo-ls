import ast
from .constants import *
from .symbol import Symbol

class Evaluation():
    """ Evaluation indicates, for a specific place how can be evaluated a specific symbol
    For example:
    a = 5
    For the symbol a, the evaluation will be:
    - type: primitive
    - instance: True
    a = Object()
    - type: Object
    - instance: True
    a = func()
    - type: func return type
    - instance: depending on func eval
    a = Object
    - type: Object
    - instance: False
    If type is not a primitive, it is a weakref to the symbol. If not, it is the symbol

    """

    def __init__(self, node = None, parentSymbol = None):
        """try to return the symbol corresponding to the expression, evaluated in the context 
        of 'symbol' (a function, class or file)."""
        if node:
            self.type, self.instance = self.evaluateAST(node, parentSymbol)
        else:
            self.type = None
            self.instance = False
        self.value = None #for primitives
        
    
    def evaluateAST(self, node, parentSymbol):
        type = None
        instance = True
        if isinstance(node, ast.Constant):
            type = Symbol("constant", SymType.PRIMITIVE, "")
        elif isinstance(node, ast.Dict):
            type = Symbol("dict", SymType.PRIMITIVE, "")
        elif isinstance(node, ast.List):
            s = Symbol("list", SymType.PRIMITIVE, "")
            res = []
            for n in node.elts:
                if not isinstance(n, ast.Constant):
                    break
                res.append(n.value)
            s.eval = Evaluation()
            s.eval.value = res
            type = s
        elif isinstance(node, ast.Call):
            f = node.func
            if isinstance(f, ast.Name):
                infered = parentSymbol.inferName(f.id, f.lineno)
                if infered and infered.eval:
                    type = infered.eval.type
                    instance = infered.eval.instance
            elif isinstance(f, ast.Attribute):
                return self.evaluateAST(f, parentSymbol)
        elif isinstance(node, ast.Attribute):
            v = self.evaluateAST(node.value, parentSymbol)
            if v and node.attr in v.symbols: #TODO wrong, don't use .symbols?
                return v.symbols[node.attr]
        elif isinstance(node, ast.Name):
            infered = parentSymbol.inferName(node.id, node.lineno)
            if infered and infered.eval:
                type = infered.eval.type
        return (type, instance)