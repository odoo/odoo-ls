import ast
import weakref
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

    def __init__(self):
        """try to return the symbol corresponding to the expression, evaluated in the context 
        of 'symbol' (a function, class or file)."""
        self.type = None
        self.instance = False
        self.value = None #for primitives

    
    def eval_import(self, target_symbol):
        """set the evaluation used in a import symbol, for a target_symbol"""
        self.type = weakref.ref(target_symbol)
        if target_symbol.type in [SymType.VARIABLE, SymType.PRIMITIVE]:
            self.instance = True
        return self
    
    def getType(self):
        if isinstance(self.type, weakref.ref):
            return self.type()
        return self.type
    
    def evalAST(self, node, parentSymbol):
        if node:
            self.type, self.instance = self._evaluateAST(node, parentSymbol)
        return self
    
    def _evaluateAST(self, node, parentSymbol):
        """evaluateAST returns for an AST node an a parent Symbol the type and if it is an instance or not.
        type is not a weakref only if the symbol is created here"""
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
                    type = weakref.ref(infered.eval.getType())
                    instance = infered.eval.instance
            elif isinstance(f, ast.Attribute):
                return self._evaluateAST(f, parentSymbol)#TODO probably wrong
        elif isinstance(node, ast.Attribute):
            sym, inst = self._evaluateAST(node.value, parentSymbol)
            if sym and node.attr in sym.symbols: #TODO wrong, don't use .symbols?
                type = sym.symbols[node.attr]
        elif isinstance(node, ast.Name):
            infered = parentSymbol.inferName(node.id, node.lineno)
            if infered and infered.eval:
                type = infered.eval.getType()
        return (type, instance)