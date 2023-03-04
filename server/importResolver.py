import glob
import os
from pathlib import Path
from .odoo import Odoo
from .symbol import Symbol

__all__ = ["loadSymbolsFromImportStmt", "resolve_packages"]

def loadSymbolsFromImportStmt(ls, file_symbol, parent_symbol, from_stmt, names, level, 
                    lineno, end_lineno):
    file_tree = resolve_packages(file_symbol, level, from_stmt)
    from_symbol = _get_or_create_symbol(ls, Odoo.get().symbols, file_tree, file_symbol, None, lineno, end_lineno)

    for name, asname in names:
        if name != '*':
            variable = Symbol(asname if asname else name, "variable", file_symbol.paths[0])
            variable.startLine = lineno
            variable.endLine = end_lineno
            variable.evaluationType = False
            parent_symbol.add_symbol(variable)
            if not from_symbol:
                continue
            from_symbol = _get_or_create_symbol(ls, from_symbol, name.split(".")[:-1], file_symbol, None, lineno, end_lineno)
            if not from_symbol:
                continue
            last_part_name = name.split(".")[-1]
            name_symbol = from_symbol.get_symbol([], [last_part_name], excl=parent_symbol) #find the last part of the name
            if not name_symbol:
                name_symbol = _resolve_new_symbol(ls, file_symbol, from_symbol, last_part_name, None, 
                                                lineno, end_lineno)
            if not name_symbol:
                continue
            variable.evaluationType = name_symbol.get_tree() if name_symbol else False
            if name_symbol and level > 0:
                if parent_symbol.get_tree() not in name_symbol.dependents:
                    name_symbol.dependents.append(parent_symbol.get_tree())
        else:
            if from_symbol:
                allowed_sym = True
                if "__all__" in from_symbol.symbols:
                    allowed_sym = from_symbol.symbols["__all__"]
                    while allowed_sym and allowed_sym.type == "variable" and isinstance(allowed_sym.evaluationType, list):
                        allowed_sym = Odoo.get().symbols.get_symbol([], allowed_sym.evaluationType)
                    if allowed_sym:
                        allowed_sym = allowed_sym.evaluationType
                        if not allowed_sym or not allowed_sym.type == "primitive" and not allowed_sym.name == "list":
                            print("debug= wrong __all__")
                            allowed_sym = True
                    if not isinstance(allowed_sym, Symbol):
                        allowed_sym = True
                for s in from_symbol.symbols.values():
                    if allowed_sym == True or s.name in allowed_sym.evaluationType:
                        variable = Symbol(s.name, "variable", file_symbol.paths[0])
                        variable.startLine = lineno
                        variable.endLine = end_lineno
                        variable.evaluationType = s.get_tree()
                        parent_symbol.add_symbol(variable)

def resolve_packages(file_symbol, level, from_stmt):
    """based on the file path and the from statement of an import statement, return the file tree
    to use in a get_symbol search"""
    file_tree = []
    if level != 0:
        if level > len(Path(file_symbol.paths[0]).parts):
            print("ERROR: level is too big ! The current path doesn't have enough parents")
            return
        if file_symbol.type == "package":
            #as the __init__.py is one level higher, we lower of 1 to match the directory level
            level -= 1
        if level == 0:
            file_tree = file_symbol.get_tree()[0]
        else:
            file_tree = file_symbol.get_tree()[0][:-level]
    file_tree += from_stmt.split(".") if from_stmt != None else []
    return file_tree

def _get_or_create_symbol(ls, symbol, names, file_symbol, asname, lineno, end_lineno):
    """try to return sub symbol that is a file or package, or create the symbol"""
    for branch in names:
        next_symbol = symbol.get_symbol([branch])
        if not next_symbol:
            next_symbol = _resolve_new_symbol(ls, file_symbol, symbol, branch, asname, 
                                                lineno, end_lineno)
        symbol = next_symbol
        if not symbol:
            break
    return symbol

def _resolve_new_symbol(ls, file_symbol, parent_symbol, name, asname, lineno, end_lineno):
    from .pythonArchBuilder import PythonArchBuilder
    if parent_symbol and parent_symbol.type == "compiled":
        #in case of compiled file, import symbols to resolve imports
        variable = Symbol(asname if asname else name, "variable", file_symbol.paths[0])
        variable.startLine = lineno
        variable.endLine = end_lineno
        variable.evaluationType = False
        return variable
    for path in parent_symbol.paths:
        full_path = os.path.join(path, name)
        if os.path.isdir(full_path):
            if parent_symbol.get_tree()[0] == ["odoo", "addons"]:
                module = parent_symbol.getModule()
                if not module:
                    """If we are searching for a odoo.addons.* element, skip it if we are not in a module.
                    It means we are in a file like odoo/*, and modules are not loaded yet."""
                    return
            parser = PythonArchBuilder(ls, full_path, parent_symbol, importMode=True)
            return parser.load_arch()
        elif os.path.isfile(full_path + ".py"):
            parser = PythonArchBuilder(ls, full_path + ".py", parent_symbol, importMode=True)
            return parser.load_arch()
        elif parent_symbol.get_tree()[0] != []: #don't try to glob on root and direct subpackages
            if os.name == "nt":
                paths = glob.glob(full_path + r".*.pyd")
                if paths:
                    sym = Symbol(name, "compiled", paths)
                    parent_symbol.add_symbol(sym)
                    return sym
            else:
                paths = glob.glob(full_path + r".*.so")
                if paths:
                    sym = Symbol(name, "compiled", paths)
                    parent_symbol.add_symbol(sym)
                    return sym
    return False
