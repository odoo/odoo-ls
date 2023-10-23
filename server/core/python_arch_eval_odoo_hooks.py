from ..constants import *
from .evaluation import Evaluation
from .odoo import Odoo
from .symbol import Symbol
from ..references import RegisteredRef


class EvaluationTestCursor(Evaluation):

    def _get_symbol_hook(self, symbol, context):
        """To be overriden for specific contextual evaluations"""
        if context and context.get("test_mode", False):
            return self.test_cursor
        return symbol


class EvaluationEnvGetItem(Evaluation):

    def _get_symbol_hook(self, symbol, context):
        from .odoo import Odoo
        model = Odoo.get().models.get(context["args"], None)
        if model:
            main_sym = model.get_main_symbols(context["module"])
            if main_sym:
                return RegisteredRef(main_sym[0])
        return None


class EvaluationTakeParent(Evaluation):

    def _get_symbol_hook(self, symbol, context):
        from .odoo import Odoo
        class_sym = context.get("parent", None)
        if not class_sym:
            return None
        return RegisteredRef(class_sym)


class PythonArchEvalOdooHooks:

    @staticmethod
    def on_file_eval(symbol):
        if symbol.get_tree() == (["odoo", "models"], []):
            baseClass = symbol.get_symbol([], ["BaseModel"])
            if baseClass:
                PythonArchEvalOdooHooks.on_baseModel_eval(baseClass)
        elif symbol.get_tree() == (["odoo", "api"], []):
            envClass = symbol.get_symbol([], ["Environment"])
            if envClass:
                PythonArchEvalOdooHooks.on_env_eval(envClass)
        elif symbol.get_tree() == (["odoo", "tests", "common"], []):
            transactionClass = symbol.get_symbol([], ["TransactionCase"])
            if transactionClass:
                PythonArchEvalOdooHooks.on_transactionCase_eval(transactionClass)

    staticmethod
    def on_baseModel_eval(symbol):
        iter = symbol.get_symbol([], ["__iter__"])
        iter.eval = EvaluationTakeParent()
        # env
        env = symbol.get_symbol([], ["env"])
        envClass = Odoo.get().get_symbol(["odoo", "api"], ["Environment"])
        if env and envClass:
            env.eval = Evaluation(
                symbol=RegisteredRef(envClass),
                instance = True
            )
            env.eval.context["test_mode"] = False # used to define the Cursor type
            env.add_dependency(envClass, BuildSteps.ARCH_EVAL, BuildSteps.ARCH)
            env.doc = ""
        #ids
        ids = symbol.get_symbol([], ["ids"])
        if ids:
            ids.eval = Evaluation()
            ids.eval._main_symbol = Symbol("list", SymType.VARIABLE)
            ids.eval._symbol = RegisteredRef(ids.eval._main_symbol)
            ids.eval.instance = True
            ids.eval.value = []
        #sudo
        sudo = symbol.get_symbol([], ["sudo"])
        if sudo:
            sudo.eval = EvaluationTakeParent()
        #create
        create = symbol.get_symbol([], ["create"])
        if create:
            create.eval = EvaluationTakeParent()
        #search
        search = symbol.get_symbol([], ["search"])
        if search:
            search.eval = EvaluationTakeParent()

    @staticmethod
    def on_env_eval(symbol):
        get_item = symbol.get_symbol([], ["__getitem__"])
        get_item.eval = EvaluationEnvGetItem()
        cr = symbol.get_symbol([], ["cr"])
        cursor_sym = Odoo.get().get_symbol(["odoo", "sql_db"], ["Cursor"])
        if cursor_sym:
            cr.eval = EvaluationTestCursor(
                symbol=RegisteredRef(cursor_sym),
                instance = True
            )
            test_cursor_sym = Odoo.get().get_symbol(["odoo", "sql_db"], ["TestCursor"])
            cr.eval.test_cursor = RegisteredRef(test_cursor_sym)
            cr.add_dependency(cursor_sym, BuildSteps.ARCH_EVAL, BuildSteps.ARCH)
            cr.doc = ""

    @staticmethod
    def on_transactionCase_eval(symbol):
        envModel = Odoo.get().get_symbol(["odoo", "api"], ["Environment"])
        env_var = symbol.get_symbol([], ["env"]) #should already exists
        if env_var and envModel:
            env_var.eval = Evaluation(
                symbol=RegisteredRef(envModel),
                instance = True
            )
            env_var.eval.context["test_mode"] = True # used to define the Cursor type
            env_var.add_dependency(envModel, BuildSteps.ARCH_EVAL, BuildSteps.ARCH)
            env_var.doc = ""
