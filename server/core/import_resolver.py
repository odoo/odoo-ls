import glob
import os
from pathlib import Path
from ..python_utils import PythonUtils
from ..constants import *
from .odoo import Odoo
from .symbol import CompiledSymbol

__all__ = ["resolve_import_stmt"]


def resolve_import_stmt(ls, source_file_symbol, parent_symbol, from_stmt, name_aliases, level,
                    start_pos, end_pos):
    """return a list of list(len=3) [[name, symbol, file_tree]] for each name in the import statement. If symbol doesn't exist,
    it will be created if possible or None will be returned.
    file_tree contains the the full file_tree to search for each name. Ex: from os import path => os
    from .test import A => tree to current file + test"""
    file_tree = _resolve_packages(source_file_symbol, level, from_stmt)
    res = [[alias, None, file_tree] for alias in name_aliases]
    from_symbol = _get_or_create_symbol(ls, Odoo.get().symbols, file_tree, source_file_symbol, None, start_pos, end_pos)
    if not from_symbol:
        return res

    name_index = -1
    for alias in name_aliases:
        name = alias.name
        name_index += 1
        if name == '*':
            res[name_index][1] = from_symbol
            continue
        found_symbol = False
        if not alias.asname:
            #if asname is not defined, we only search for the first part of the name.
            #In all "from X import A case", it simply means search for A
            #But in "import A.B.C", it means search for A only.
            #If user typed import A.B.C as D, we will search for A.B.C to link it to symbol D,
            #but if user typed import A.B.C, we will only search for A and create A, as any use by after will require to type A.B.C
            name_symbol = _get_or_create_symbol(ls, from_symbol, [name.split(".")[0]], source_file_symbol, None, start_pos, end_pos)
            if not name_symbol: #If not a file/package, try to look up in symbols in current file (second parameter of get_symbol)
                if "." not in name: #if the first element is also the last one, check in local symbols
                    name_symbol = from_symbol.get_symbol([], [name.split(".")[0]]) #find the last part of the name
                if not name_symbol:
                    continue
            res[name_index][1] = name_symbol
            found_symbol = True
            #do not stop here, we still want to import the full name, even if we only store the first now
        #get the full file_tree, including the first part of the name import stmt. (os in import os.path)
        next_symbol = _get_or_create_symbol(ls, from_symbol, name.split(".")[:-1], source_file_symbol, None, start_pos, end_pos)
        if not next_symbol:
            continue
        #now we can search for the last symbol, or create it if it doesn't exist
        last_part_name = name.split(".")[-1]
        name_symbol = _get_or_create_symbol(ls, next_symbol, [last_part_name], source_file_symbol, None, start_pos, end_pos)
        if not name_symbol: #If not a file/package, try to look up in symbols in current file (second parameter of get_symbol)
            name_symbol = next_symbol.get_symbol([], [last_part_name]) #find the last part of the name
            if not name_symbol:
                continue
        #we found it ! store the result if not already done
        if not found_symbol:
            res[name_index][1] = name_symbol
    return res

def find_module(ls, name):
    from .python_arch_builder import PythonArchBuilder
    odoo_addons = Odoo.get().get_symbol(["odoo", "addons"], [])
    for path in odoo_addons.get_paths():
        full_path = os.path.join(path, name)
        if PythonUtils.is_dir_cs(full_path):
            parser = PythonArchBuilder(ls, odoo_addons, full_path)
            sym = parser.load_arch(require_module=True)
            if sym:
                return sym
    return None

def _resolve_packages(file_symbol, level, from_stmt):
    """based on the file path and the from statement of an import statement, return the file tree
    to use in a get_symbol search"""
    file_tree = []
    if level != 0:
        if level > len(Path(file_symbol.get_paths()[0]).parts):
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

def _get_or_create_symbol(ls, symbol, names, file_symbol, asname, start_pos, end_pos):
    """try to return sub symbol that is a file or package, or create the symbol"""
    for branch in names:
        next_symbol = symbol.get_symbol([branch])
        if not next_symbol:
            next_symbol = _resolve_new_symbol(ls, file_symbol, symbol, branch, asname,
                                                start_pos, end_pos)
        symbol = next_symbol
        if not symbol:
            break
    return symbol

def _resolve_new_symbol(ls, file_symbol, parent_symbol, name, asname, start_pos, end_pos):
    """ Return a new symbol for the name and given parent_Symbol, that is matching what is on disk"""
    from .python_arch_builder import PythonArchBuilder
    if parent_symbol and parent_symbol.type == SymType.COMPILED:
        #in case of compiled file, import symbols to resolve imports
        variable = CompiledSymbol(asname if asname else name, "")
        variable.start_pos = start_pos
        variable.end_pos = end_pos
        variable.eval = None
        return variable
    for path in parent_symbol.paths:
        full_path = os.path.join(path, name)
        if path == Odoo.get().stubs_dir:
            #stubs file un typeshed are in a second directory in the same path
            full_path = os.path.join(full_path, name)
        if PythonUtils.is_dir_cs(full_path):
            parser = PythonArchBuilder(ls, parent_symbol, full_path)
            return parser.load_arch()
        elif PythonUtils.is_file_cs(full_path + ".py"):
            parser = PythonArchBuilder(ls, parent_symbol, full_path + ".py")
            return parser.load_arch()
        elif PythonUtils.is_file_cs(full_path + ".pyi"):
            parser = PythonArchBuilder(ls, parent_symbol, full_path + ".pyi")
            return parser.load_arch()
        elif parent_symbol.get_tree()[0] != []: #don't try to glob on root and direct subpackages
            if os.name == "nt":
                paths = glob.glob(full_path + r".*.pyd")
                if paths:
                    sym = CompiledSymbol(name, paths)
                    parent_symbol.add_symbol(sym)
                    return sym
            else:
                paths = glob.glob(full_path + r".*.so")
                if paths:
                    sym = CompiledSymbol(name, paths)
                    parent_symbol.add_symbol(sym)
                    return sym
    return False
