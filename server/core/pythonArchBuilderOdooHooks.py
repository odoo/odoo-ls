import weakref
from server.core.symbol import Symbol
from server.constants import SymType
from server.core.evaluation import Evaluation

class PythonArchBuilderOdooHooks:

    @staticmethod
    def on_class_declaration(symbol):
        """ called when ArchBuilder create a new class Symbol """
        from server.core.odoo import Odoo
        if symbol.name == "BaseModel": #fast, basic check
            if symbol.get_tree() == (["odoo", "models"], ["BaseModel"]): #slower but more precise verification
                # ---------- env ----------
                envModel = Odoo.get().get_symbol(["odoo", "api"], ["Environment"])
                env_var = Symbol("env", SymType.VARIABLE, symbol.paths)
                slot_sym = symbol.get_symbol([], ["__slots__"])
                if not slot_sym:
                    return #TODO should never happen
                env_var.startLine = slot_sym.startLine
                env_var.endLine = slot_sym.endLine
                if envModel:
                    env_var.eval = Evaluation(
                        symbol=weakref.ref(envModel),
                        instance = True
                    )
                    envModel.arch_dependents.add(env_var)
                    env_var.doc = ""
                symbol.add_symbol(env_var)
                # ---------- env.cr ----------
                cr_var = Symbol("cr", SymType.VARIABLE, envModel.paths)
                if envModel:
                    cr_var.startLine = envModel.startLine
                    cr_var.endLine = envModel.endLine
                cursor_sym = Odoo.get().get_symbol(["odoo", "sql_db"], ["Cursor"])
                if cursor_sym:
                    cr_var.eval = Evaluation(
                        symbol=weakref.ref(cursor_sym),
                        instance = True
                    )
                    cursor_sym.arch_dependents.add(cr_var)
                    cr_var.doc = ""
                envModel.add_symbol(cr_var)
                # ---------- env.uid ----------
                cr_var = Symbol("uid", SymType.VARIABLE, envModel.paths)
                if envModel:
                    cr_var.startLine = envModel.startLine
                    cr_var.endLine = envModel.endLine
                cr_var.doc = "the current user id (for access rights checks)"
                envModel.add_symbol(cr_var)
                # ---------- env.context ----------
                context_var = Symbol("context", SymType.VARIABLE, envModel.paths)
                if envModel:
                    context_var.startLine = envModel.startLine
                    context_var.endLine = envModel.endLine
                context_var.doc = "the current context dictionary (arbitrary metadata)"
                envModel.add_symbol(context_var)
                # ---------- env.su ----------
                attr_var = Symbol("context", SymType.VARIABLE, envModel.paths)
                if envModel:
                    attr_var.startLine = envModel.startLine
                    attr_var.endLine = envModel.endLine
                attr_var.doc = "whether in superuser mode"
                envModel.add_symbol(attr_var)