import glob
import os
from pathlib import Path
import weakref
from ..constants import *
from .odoo import Odoo
from .symbol import Symbol

__all__ = ["resolve_import_stmt"]

def resolve_import_stmt(ls, source_file_symbol, parent_symbol, from_stmt, name_aliases, level, 
                    lineno, end_lineno):
    """return a list of list(len=4) [[name, asname, symbol, file_tree]] for each name in the import statement. If symbol doesn't exist, 
    it will be created if possible or None will be returned.
    file_tree contains the the full file_tree to search for each name. Ex: from os import path => os
    from .test import A => tree to current file + test"""
    file_tree = _resolve_packages(source_file_symbol, level, from_stmt)
    res = [[alias, None, file_tree] for alias in name_aliases]
    from_symbol = _get_or_create_symbol(ls, Odoo.get().symbols, file_tree, source_file_symbol, None, lineno, end_lineno)
    if not from_symbol:
        return res

    name_index = -1
    for alias in name_aliases:
        name = alias.name
        name_index += 1
        if name == '*':
            res[name_index][1] = from_symbol
            continue
        #get the full file_tree, including the first part of the name import stmt. (os in import os.path) 
        next_symbol = _get_or_create_symbol(ls, from_symbol, name.split(".")[:-1], source_file_symbol, None, lineno, end_lineno)
        if not next_symbol:
            continue
        #now we can search for the last symbol, or create it if it doesn't exist
        last_part_name = name.split(".")[-1]
        name_symbol = next_symbol.get_symbol([], [last_part_name], excl=parent_symbol) #find the last part of the name
        if not name_symbol:
            name_symbol = _resolve_new_symbol(ls, source_file_symbol, next_symbol, last_part_name, None, 
                                            lineno, end_lineno)
        if not name_symbol:
            continue
        #we found it ! store the result
        res[name_index][1] = name_symbol
    return res

def _resolve_packages(file_symbol, level, from_stmt):
    """based on the file path and the from statement of an import statement, return the file tree
    to use in a get_symbol search"""
    file_tree = []
    if level != 0:
        if level > len(Path(file_symbol.paths[0]).parts):
            print("ERROR: level is too big ! The current path doesn't have enough parents")
            return
        if file_symbol.type == SymType.PACKAGE:
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
    """ Return a new symbol for the name and given parent_Symbol, that is matching what is on disk"""
    from .pythonArchBuilder import PythonArchBuilder
    if parent_symbol and parent_symbol.type == SymType.COMPILED:
        #in case of compiled file, import symbols to resolve imports
        variable = Symbol(asname if asname else name, SymType.VARIABLE, file_symbol.paths[0])
        variable.startLine = lineno
        variable.endLine = end_lineno
        variable.eval = None
        return variable
    for path in parent_symbol.paths:
        full_path = os.path.join(path, name)
        if os.path.isdir(full_path):
            if parent_symbol.get_tree()[0] == ["odoo", "addons"]:
                module = parent_symbol.get_module()
                if not module:
                    """If we are searching for a odoo.addons.* element, skip it if we are not in a module.
                    It means we are in a file like odoo/*, and modules are not loaded yet."""
                    return
            parser = PythonArchBuilder(ls, parent_symbol, full_path)
            return parser.load_arch()
        elif os.path.isfile(full_path + ".py"):
            parser = PythonArchBuilder(ls, parent_symbol, full_path + ".py")
            return parser.load_arch()
        elif parent_symbol.get_tree()[0] != []: #don't try to glob on root and direct subpackages
            if os.name == "nt":
                paths = glob.glob(full_path + r".*.pyd")
                if paths:
                    sym = Symbol(name, SymType.COMPILED, paths)
                    parent_symbol.add_symbol(sym)
                    return sym
            else:
                paths = glob.glob(full_path + r".*.so")
                if paths:
                    sym = Symbol(name, SymType.COMPILED, paths)
                    parent_symbol.add_symbol(sym)
                    return sym
    return False