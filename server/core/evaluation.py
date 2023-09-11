import ast
from ..constants import *
from .symbol import Symbol
from server.references import RegisteredRef


class Evaluation():
    """ Evaluation indicates, for a specific place how can be evaluated a specific symbol
    For example:
    a = 5
    For the symbol a, the evaluation will be:
    - symbol: Symbol
    - instance: True
    a = Object()
    - symbol: RegisteredRef of Symbol of Object
    - instance: True
    a = func()
    - symbol: RegisteredRef to symbol of func return type
    - instance: depending on func eval
    a = Object
    - symbol: RegisteredRef of Symbol of Object
    - instance: False
    a = func
    - symbol: RegisteredRef of Symbol of Object
    - instance: False
    If symbol is not a primitive, it is a RegisteredRef to the symbol. If not, it is the symbol

    """

    def __init__(self, symbol=None, instance=False, value=None):
        """try to return the symbol corresponding to the expression, evaluated in the context
        of 'symbol' (a function, class or file)."""
        self._symbol = symbol
        self.instance = instance
        self.context = {} #evaluation context
        self.value = value #for primitives
        self._symbol_main = None #to hold ref for local symbols

    @property
    def symbol(self):
        raise NotImplementedError

    @symbol.setter
    def symbol(self, value):
        self._symbol = value

    def eval_import(self, target_symbol):
        """set the evaluation used in a import symbol, for a target_symbol"""
        self._symbol = RegisteredRef(target_symbol)
        if target_symbol.type in [SymType.VARIABLE, SymType.PRIMITIVE]:
            self.instance = True
        return self

    def get_symbol_rr(self, context = None):
        return self._get_symbol_hook(self._symbol, context)

    def get_symbol(self, context = None):
        """ context_sym is a symbol that is used to defined the evaluation. Usually the symbol that hold the evaluation"""
        rr = self.get_symbol_rr(context)
        if not rr:
            return None
        return rr.ref

    def _get_symbol_hook(self, rr_symbol, context):
        """To be overriden for specific contextual evaluations"""
        return rr_symbol

    def evalAST(self, node, parentSymbol):
        if node:
            self._symbol, self.instance, self.context = self._evaluateAST(node, parentSymbol)
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
        symbol is always a RegisteredRef"""
        symbol = None
        instance = True
        if isinstance(node, ast.Constant):
            self._symbol_main = Symbol("constant", SymType.PRIMITIVE)
            self._symbol_main.value = node.value
            symbol = RegisteredRef(self._symbol_main)
        elif isinstance(node, ast.Dict):
            self._symbol_main = Symbol("dict", SymType.PRIMITIVE)
            self._symbol_main.value = self._extract_literal_dict(node)
            symbol = RegisteredRef(self._symbol_main)
        elif isinstance(node, ast.List) or isinstance(node, ast.Tuple):
            self._symbol_main = Symbol("list", SymType.PRIMITIVE)
            if isinstance(node, ast.Tuple):
                self._symbol_main.name = "tuple"
            res = []
            for n in node.elts:
                if not isinstance(n, ast.Constant):
                    break
                res.append(n.value)
            self._symbol_main.value = res
            symbol = RegisteredRef(self._symbol_main)
        elif isinstance(node, ast.Call):
            f = node.func
            #1: get object to call
            base_ref, inst, ctxt = self._evaluateAST(f, parentSymbol)
            if not base_ref or not base_ref.ref:
                return (None, False, {})
            base = base_ref.ref
            if base.type == SymType.CLASS and inst == False:
                return base_ref, True, base.get_context(node.args, node.keywords)
            elif base.type == SymType.CLASS and inst == True:
                call_func = base.symbols.get("__call__", None)
                if not call_func or not call_func.eval:
                    return (None, False, {})
                return call_func.eval.get_symbol_rr(), call_func.eval.instance, {}
            elif base.type == SymType.FUNCTION and base.eval:
                return base.eval.get_symbol_rr(), base.eval.instance, {}
            #TODO other types are errors?
        elif isinstance(node, ast.Attribute):
            v, instance, ctxt = self._evaluateAST(node.value, parentSymbol)
            if not v:
                return (None, False, {})
            v = v.ref
            base, instance = v.follow_ref()
            attribute = v.symbols.get(node.attr, None)
            if not attribute:
                return (None, False, {})
            return RegisteredRef(attribute), attribute.type == SymType.VARIABLE, {}
        elif isinstance(node, ast.Name):
            infered_sym = parentSymbol.inferName(node.id, node.lineno)
            if not infered_sym:
                return (None, False, {})
            symbol, instance = infered_sym.follow_ref()
            symbol = RegisteredRef(symbol)
        return (symbol, instance, {})