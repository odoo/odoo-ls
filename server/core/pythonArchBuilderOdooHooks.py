from server.references import RegisteredRefSet, RegisteredRef
from server.core.symbol import Symbol
from server.constants import SymType
from server.core.evaluation import Evaluation


class PythonArchBuilderOdooHooks:

    @staticmethod
    def on_module_declaration(symbol):
        if symbol.name == "logging":
            if symbol.get_tree() == (["logging"], []):
                get_logger = symbol.get_symbol([], ["getLogger"])
                logger = symbol.get_symbol([], ["Logger"])
                if get_logger and logger:
                    get_logger.eval = Evaluation(
                        symbol=RegisteredRef(logger),
                        instance = True
                    )

    @staticmethod
    def on_class_declaration(symbol):
        """ called when ArchBuilder create a new class Symbol """
        from server.core.odoo import Odoo
        if symbol.name == "BaseModel": #fast, basic check
            if symbol.get_tree() == (["odoo", "models"], ["BaseModel"]): #slower but more precise verification
                # ---------- env ----------
                env_var = Symbol("env", SymType.VARIABLE, symbol.paths)
                slot_sym = symbol.get_symbol([], ["__slots__"])
                if not slot_sym:
                    return #TODO should never happen
                env_var.startLine = slot_sym.startLine
                env_var.endLine = slot_sym.endLine
                symbol.add_symbol(env_var)
        if symbol.name == "Environment": #fast, basic check
            if symbol.get_tree() == (["odoo", "api"], ["Environment"]): #slower but more precise verification
                # ---------- env.cr ----------
                cr_var = Symbol("cr", SymType.VARIABLE, symbol.paths)
                if symbol:
                    cr_var.startLine = symbol.startLine
                    cr_var.endLine = symbol.endLine
                symbol.add_symbol(cr_var)
                # ---------- env.uid ----------
                cr_var = Symbol("uid", SymType.VARIABLE, symbol.paths)
                if symbol:
                    cr_var.startLine = symbol.startLine
                    cr_var.endLine = symbol.endLine
                cr_var.doc = "the current user id (for access rights checks)"
                symbol.add_symbol(cr_var)
                # ---------- env.context ----------
                context_var = Symbol("context", SymType.VARIABLE, symbol.paths)
                if symbol:
                    context_var.startLine = symbol.startLine
                    context_var.endLine = symbol.endLine
                context_var.doc = "the current context dictionary (arbitrary metadata)"
                symbol.add_symbol(context_var)
                # ---------- env.su ----------
                attr_var = Symbol("su", SymType.VARIABLE, symbol.paths)
                if symbol:
                    attr_var.startLine = symbol.startLine
                    attr_var.endLine = symbol.endLine
                attr_var.doc = "whether in superuser mode"
                symbol.add_symbol(attr_var)