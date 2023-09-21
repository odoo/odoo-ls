import ast
from server.references import RegisteredRefSet, RegisteredRef
from server.core.symbol import Symbol
from server.constants import SymType
from server.core.evaluation import Evaluation


def Relational_get_context(self, args, keywords):
    comodel_name = ""
    if args and isinstance(args[0], ast.Constant):
        comodel_name = args[0].value
    else:
        for kw in keywords:
            if kw.arg == "comodel_name":
                if isinstance(kw.value, ast.Constant):
                    comodel_name = kw.value.value
                elif isinstance(kw.value, ast.Name):
                    comodel_name = kw.value.id
    if not comodel_name:
        #sometimes comodel is not set because this is an override of an existing field. Skip the context in this case
        return {}
    return {"comodel_name": comodel_name}


class PythonArchBuilderOdooHooks:

    @staticmethod
    def on_package_declaration(symbol):
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
                env_var = Symbol("env", SymType.VARIABLE)
                slot_sym = symbol.get_symbol([], ["__slots__"])
                if not slot_sym:
                    return #TODO should never happen
                env_var.start_pos = slot_sym.start_pos
                env_var.end_pos = slot_sym.end_pos
                symbol.add_symbol(env_var)
        if symbol.name == "Environment": #fast, basic check
            if symbol.get_tree() == (["odoo", "api"], ["Environment"]): #slower but more precise verification
                ref_to_new = symbol.get_symbol([], ["__new__"])
                if not ref_to_new:
                    ref_to_new = symbol
                # ---------- env.cr ----------
                cr_var = Symbol("cr", SymType.VARIABLE)
                if ref_to_new:
                    cr_var.start_pos = ref_to_new.start_pos
                    cr_var.end_pos = ref_to_new.end_pos
                symbol.add_symbol(cr_var)
                # ---------- env.uid ----------
                cr_var = Symbol("uid", SymType.VARIABLE)
                if ref_to_new:
                    cr_var.start_pos = ref_to_new.start_pos
                    cr_var.end_pos = ref_to_new.end_pos
                cr_var.doc = Symbol("str", SymType.PRIMITIVE)
                cr_var.doc.value = "the current user id (for access rights checks)"
                symbol.add_symbol(cr_var)
                # ---------- env.context ----------
                context_var = Symbol("context", SymType.VARIABLE)
                if ref_to_new:
                    context_var.start_pos = ref_to_new.start_pos
                    context_var.end_pos = ref_to_new.end_pos
                context_var.doc = Symbol("str", SymType.PRIMITIVE)
                context_var.doc.value = "the current context dictionary (arbitrary metadata)"
                symbol.add_symbol(context_var)
                # ---------- env.su ----------
                attr_var = Symbol("su", SymType.VARIABLE)
                if ref_to_new:
                    attr_var.start_pos = ref_to_new.start_pos
                    attr_var.end_pos = ref_to_new.end_pos
                attr_var.doc = Symbol("str", SymType.PRIMITIVE)
                attr_var.doc.value = "whether in superuser mode"
                symbol.add_symbol(attr_var)
        elif symbol.name in ["Many2one", "Many2many", "One2many"]:
            if symbol.get_tree() in [(["odoo", "fields"], ["Many2one"]),
                                     (["odoo", "fields"], ["Many2many"]),
                                     (["odoo", "fields"], ["One2many"]),]:
                symbol.get_context = Relational_get_context.__get__(symbol, symbol.__class__)