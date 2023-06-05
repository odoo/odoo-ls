import ast
import weakref
from ..constants import *
from .symbol import Symbol


class Evaluation():
    """ Evaluation indicates, for a specific place how can be evaluated a specific symbol
    For example:
    a = 5
    For the symbol a, the evaluation will be:
    - symbol: Symbol
    - instance: True
    a = Object()
    - symbol: weakref of Symbol of Object
    - instance: True
    a = func()
    - symbol: weakref to symbol of func return type
    - instance: depending on func eval
    a = Object
    - symbol: weakref of Symbol of Object
    - instance: False
    a = func
    - symbol: weakref of Symbol of Object
    - instance: False
    If symbol is not a primitive, it is a weakref to the symbol. If not, it is the symbol

    """

    def __init__(self, symbol=None, instance=False, value=None):
        """try to return the symbol corresponding to the expression, evaluated in the context 
        of 'symbol' (a function, class or file)."""
        self.symbol = symbol
        self.instance = instance
        self.value = value #for primitives
        self._symbol = None #to hold ref for local symbols

    
    def eval_import(self, target_symbol):
        """set the evaluation used in a import symbol, for a target_symbol"""
        self.symbol = weakref.ref(target_symbol)
        if target_symbol.type in [SymType.VARIABLE, SymType.PRIMITIVE]:
            self.instance = True
        return self
    
    def getSymbol(self):
        if not self.symbol or not self.symbol():
            return None
        return self.symbol()
    
    def evalAST(self, node, parentSymbol):
        if node:
            self.symbol, self.instance = self._evaluateAST(node, parentSymbol)
        return self
    
    def _extract_literal_dict(self, node):
        res = {}
        for k, v in zip(node.keys, node.values):
            if not isinstance(k, ast.Constant) or not isinstance(v, ast.Constant):
                return None
            res[k.value] = v.value
        return res
    
    def _evaluateAST(self, node, parentSymbol):
        """evaluateAST returns for an AST node and a parent Symbol the symbol and if it is an instance or not.
        symbol is always a weakref"""
        symbol = None
        instance = True
        if isinstance(node, ast.Constant):
            self._symbol = Symbol("constant", SymType.PRIMITIVE, "")
            self._symbol.eval = Evaluation()
            self._symbol.eval.value = node.value
            symbol = weakref.ref(self._symbol)
        elif isinstance(node, ast.Dict):
            self._symbol = Symbol("dict", SymType.PRIMITIVE, "")
            self._symbol.eval = Evaluation()
            self._symbol.eval.value = self._extract_literal_dict(node)
            symbol = weakref.ref(self._symbol)
        elif isinstance(node, ast.List):
            self._symbol = Symbol("list", SymType.PRIMITIVE, "")
            res = []
            for n in node.elts:
                if not isinstance(n, ast.Constant):
                    break
                res.append(n.value)
            self._symbol.eval = Evaluation()
            self._symbol.eval.value = res
            symbol = weakref.ref(self._symbol)
        elif isinstance(node, ast.Call):
            f = node.func
            #1: get object to call
            base_ref, inst = self._evaluateAST(f, parentSymbol)
            if not base_ref or not base_ref():
                return (None, False)
            base = base_ref()
            if base.type == SymType.CLASS and inst == False:
                return base_ref, True
            elif base.type == SymType.CLASS and inst == True:
                call_func = base.symbols.get("__call__", None)
                if not call_func or not call_func.eval:
                    return (None, False)
                return call_func.eval.symbol, call_func.eval.instance
            elif base.type == SymType.FUNCTION and base.eval:
                return base.eval.symbol, base.eval.instance
            #TODO other types are errors?
        elif isinstance(node, ast.Attribute):
            v, instance = self._evaluateAST(node.value, parentSymbol)
            if not v or not v():
                return (None, False)
            v = v()
            base, instance = v.follow_ref()
            attribute = v.symbols.get(node.attr, None)
            if not attribute:
                return (None, False)
            return weakref.ref(attribute), attribute.type == SymType.VARIABLE
        elif isinstance(node, ast.Name):
            infered_sym = parentSymbol.inferName(node.id, node.lineno)
            if not infered_sym:
                return (None, False)
            symbol, instance = infered_sym.follow_ref()
            symbol = weakref.ref(symbol)
        return (symbol, instance)