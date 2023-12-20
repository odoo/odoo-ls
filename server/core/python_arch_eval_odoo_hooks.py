import ast
from typing import Any
from ..constants import *
from .evaluation import Evaluation
from .odoo import Odoo
from .symbol import Symbol
from ..references import RegisteredRef
from .import_resolver import *
from ..odoo_language_server import OdooLanguageServer


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


class EvaluationGet(Evaluation):

    def _get_symbol_hook(self, rr_symbol, context):
        parent_instance = context and context.get("parent_instance", False)
        if not parent_instance:
            return None
        return super()._get_symbol_hook(rr_symbol, context)


class EvaluationRelational(Evaluation):

    def _get_symbol_hook(self, rr_symbol, context):
        model = Odoo.get().models.get(context["comodel_name"], None)
        if model:
            main_sym = model.get_main_symbols() #module) #TODO use module in context
            if main_sym and len(main_sym) == 1:
                return RegisteredRef(main_sym[0])
        return None


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
            form_sym = symbol.get_symbol([], ["Form"])
            if form_sym:
                PythonArchEvalOdooHooks.on_form_eval(form_sym)
            transactionClass = symbol.get_symbol([], ["TransactionCase"])
            if transactionClass:
                PythonArchEvalOdooHooks.on_transactionCase_eval(transactionClass)
        elif symbol.get_tree() == (["odoo", "fields"], []):
            booleanClass = symbol.get_symbol([], ["Boolean"])
            if booleanClass:
                PythonArchEvalOdooHooks._update_get_eval(booleanClass, (["builtins"], ["bool"]))
            intClass = symbol.get_symbol([], ["Integer"])
            if intClass:
                PythonArchEvalOdooHooks._update_get_eval(intClass, (["builtins"], ["int"]))
            floatClass = symbol.get_symbol([], ["Float"])
            if floatClass:
                PythonArchEvalOdooHooks._update_get_eval(floatClass, (["builtins"], ["float"]))
            monetaryClass = symbol.get_symbol([], ["Monetary"])
            if monetaryClass:
                PythonArchEvalOdooHooks._update_get_eval(monetaryClass, (["builtins"], ["float"]))
            charClass = symbol.get_symbol([], ["Char"])
            if charClass:
                PythonArchEvalOdooHooks._update_get_eval(charClass, (["builtins"], ["str"]))
            textClass = symbol.get_symbol([], ["Text"])
            if textClass:
                PythonArchEvalOdooHooks._update_get_eval(textClass, (["builtins"], ["str"]))
            htmlClass = symbol.get_symbol([], ["Html"])
            if htmlClass:
                PythonArchEvalOdooHooks._update_get_eval(htmlClass, (["markupsafe"], ["Markup"]))
            dateClass = symbol.get_symbol([], ["Date"])
            if dateClass:
                PythonArchEvalOdooHooks._update_get_eval(dateClass, (["datetime"], ["date"]))
            datetimeClass = symbol.get_symbol([], ["Datetime"])
            if datetimeClass:
                PythonArchEvalOdooHooks._update_get_eval(datetimeClass, (["datetime"], ["datetime"]))
            binaryClass = symbol.get_symbol([], ["Binary"])
            if binaryClass:
                PythonArchEvalOdooHooks._update_get_eval(binaryClass, (["builtins"], ["bytes"]))
            imageClass = symbol.get_symbol([], ["Image"])
            if imageClass:
                PythonArchEvalOdooHooks._update_get_eval(imageClass, (["builtins"], ["bytes"]))
            selectionClass = symbol.get_symbol([], ["Selection"])
            if selectionClass:
                PythonArchEvalOdooHooks._update_get_eval(selectionClass, (["builtins"], ["str"]))
            referenceClass = symbol.get_symbol([], ["Reference"])
            if referenceClass:
                PythonArchEvalOdooHooks._update_get_eval(referenceClass, (["builtins"], ["str"]))
            jsonClass = symbol.get_symbol([], ["Json"])
            if jsonClass:
                PythonArchEvalOdooHooks._update_get_eval(jsonClass, (["builtins"], ["object"]))
            propertiesClass = symbol.get_symbol([], ["Properties"])
            if propertiesClass:
                PythonArchEvalOdooHooks._update_get_eval(propertiesClass, (["builtins"], ["object"]))
            propertiesDefinitionClass = symbol.get_symbol([], ["PropertiesDefinition"])
            if propertiesDefinitionClass:
                PythonArchEvalOdooHooks._update_get_eval(propertiesDefinitionClass, (["builtins"], ["object"]))
            idClass = symbol.get_symbol([], ["Id"])
            if idClass:
                PythonArchEvalOdooHooks._update_get_eval(idClass, (["builtins"], ["int"]))
            many2oneClass = symbol.get_symbol([], ["Many2one"])
            if many2oneClass:
                PythonArchEvalOdooHooks._update_get_eval_relational(many2oneClass)
            one2manyClass = symbol.get_symbol([], ["One2many"])
            if one2manyClass:
                PythonArchEvalOdooHooks._update_get_eval_relational(one2manyClass)
            many2manyClass = symbol.get_symbol([], ["Many2many"])
            if many2manyClass:
                PythonArchEvalOdooHooks._update_get_eval_relational(many2manyClass)

    @staticmethod
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

    @staticmethod
    def on_form_eval(symbol):
        if not Odoo.get().full_version >= "16.3":
            return
        fileSymbol = symbol.get_in_parents([SymType.FILE])
        symbols = resolve_import_stmt(OdooLanguageServer.get(), fileSymbol, fileSymbol, "form", [ast.alias("Form", "")], 1, symbol.start_pos, symbol.end_pos)
        _, found, form_symbol, _ = symbols[0]
        if found:
            symbol.eval = Evaluation(
                symbol = RegisteredRef(form_symbol),
                instance=True
            )

    @staticmethod
    def _update_get_eval(symbol, return_tree):
        get_sym = symbol.get_symbol([], ["__get__"])
        if not get_sym:
            return
        return_sym = Odoo.get().get_symbol(return_tree[0], return_tree[1])
        if not return_sym:
            symbol.not_found_paths.append((BuildSteps.ARCH_EVAL, return_tree[0] + return_tree[1]))
            Odoo.get().not_found_symbols.add(symbol)
            return
        var_sym = Symbol("returned_value", SymType.PRIMITIVE)
        var_sym.eval = Evaluation(
            symbol=RegisteredRef(return_sym),
            instance = True
        )
        get_sym.eval = EvaluationGet(
            symbol=RegisteredRef(var_sym),
            instance = True
        )
        get_sym.eval._symbol_main = var_sym
        get_sym.eval.value = ""

    def _update_get_eval_relational(symbol):
        get_sym = symbol.get_symbol([], ["__get__"])
        if not get_sym:
            return
        get_sym.eval = EvaluationRelational(
            symbol=None,
            instance = True
        )
        get_sym.eval.value = ""
