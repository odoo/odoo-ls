import ast

class PythonOdooBuilder(ast.NodeVisitor):
    """The Python Odoo Builder is the step that extracts Odoo models info for the validation.
    It represents data that are loaded and built by Odoo at loading time (model declarations, etc...)
    and that can't be used in a classic linter, due to their dynamic nature.
    This step can't be merged with Arch builder because this construction should be able to be run
    regularly like the validation, but we don't need to reload all symbols, as the file didn't change.
    In the same logic, we can't merge this step with the validation as the validation need to have all
    data coming from the simulated running odoo to work properly, so it must be done at an earlier stage.
    """

    def __init__(self, ls, symbol):
        """Prepare an odoo builder to parse the symbol"""
        self.ls = ls
        self.symbol = symbol
        self.diagnostics = []

    def load_odoo_content(self):
        self.diagnostics = []
        if self.symStack[0].validationStatus:
            return
        if self.symStack[0].type in ['namespace']:
            return