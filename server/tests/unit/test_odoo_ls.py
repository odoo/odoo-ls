import asyncio
import io
import json
import threading
import time
import weakref

import pytest
from lsprotocol.types import (DidChangeTextDocumentParams, VersionedTextDocumentIdentifier, RenameFilesParams, FileRename)
from pygls.server import StdOutTransportAdapter
from pygls.workspace import Document, Workspace

from ...server import (
    _did_change_after_delay,
    did_rename_files
)
from ...fileMgr import FileMgr
from .setup import *
from ...odoo import Odoo
from ...symbol import Symbol
from ...constants import *

"""
XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

To run / setup tests, please see setup.py

XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
"""

Odoo.import_odoo_addons = False
Odoo.get(server)

def _reset_mocks(stdin=None, stdout=None):
    stdin = stdin or io.StringIO()
    stdout = stdout or io.StringIO()

    server.lsp.transport = StdOutTransportAdapter(stdin, stdout)
    server.publish_diagnostics.reset_mock()
    server.show_message.reset_mock()
    server.show_message_log.reset_mock()

def search_in_local(symbol, name):
    found_in_local = False
    for sym in symbol.localSymbols:
        if sym.name == name:
            found_in_local = True
            break
    return found_in_local

def get_uri(path):
    #return an uri from the "tests" level with a path like ["data", "module1"]
    file_uri = pathlib.Path(__file__).parent.parent.resolve()
    file_uri = os.path.join(file_uri, *path)
    return FileMgr.pathname2uri(file_uri)

def test_load_modules():
    assert Odoo.get().symbols.get_symbol(["odoo", "addons", "module_2"]), "OdooLS Test Module2 has not been loaded from custom addons path"
    assert not Odoo.get().symbols.get_symbol(["odoo", "addons", "not_a_module"]), "NotAModule is present in symbols, but it should not have been loaded"

def test_imports():
    assert not Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "not_loaded"])
    assert not Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "not_loaded", "not_loaded_file"])
    assert not Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "not_loaded", "not_loaded_file"], ["NotLoadedClass"])
    assert not Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "not_loaded", "not_loaded_file"], ["NotLoadedFunc"])
    assert Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models"])
    model_package = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models"])
    assert "base_test_models" in model_package.symbols
    assert "base_test_models" in model_package.moduleSymbols
    assert model_package.moduleSymbols["base_test_models"] == model_package.get_symbol(["base_test_models"])
    assert model_package.symbols["base_test_models"] == model_package.get_symbol([], ["base_test_models"])
    base_test_models = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"])
    assert "CONSTANT_1" in base_test_models.symbols
    assert "CONSTANT_2" in base_test_models.symbols
    assert not "CONSTANT_3" in base_test_models.symbols
    assert "api" in base_test_models.symbols
    assert "fields" in base_test_models.symbols
    assert "models" in base_test_models.symbols
    assert "_" in base_test_models.symbols
    assert "tools" in base_test_models.symbols
    assert "BaseTestModel" in base_test_models.symbols
    constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
    assert "CONSTANT_1" in constants_dir.symbols
    assert "CONSTANT_2" in constants_dir.symbols
    assert not "CONSTANT_3" in constants_dir.symbols
    constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
    assert "CONSTANT_1" in constants_data_dir.symbols
    assert search_in_local(constants_data_dir, "CONSTANT_2")
    assert "CONSTANT_2" in constants_data_dir.symbols
    assert not "CONSTANT_3" in constants_data_dir.symbols

def test_load_classes():
    base_class = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"], ["BaseTestModel"])
    assert base_class, "BaseTestModel has not been loaded"
    assert base_class.name == "BaseTestModel"
    assert "test_int" in base_class.symbols
    assert "get_test_int" in base_class.symbols
    assert "get_constant" in base_class.symbols

def test_evaluation():
    constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "constants"])
    evaluation = constants_data_file.get_symbol([], ["CONSTANT_1"]).eval
    assert evaluation and evaluation.getSymbol()
    assert isinstance(evaluation.symbol, weakref.ref)
    assert evaluation.getSymbol().type == SymType.PRIMITIVE
    assert evaluation.instance == True
    evaluation = constants_data_file.get_symbol([], ["__all__"]).eval
    assert evaluation and evaluation.getSymbol()
    assert isinstance(evaluation.symbol, weakref.ref)
    assert evaluation.getSymbol().type == SymType.PRIMITIVE
    assert evaluation.instance == True
    assert evaluation.getSymbol().name == "list"
    assert evaluation.getSymbol().eval.value == ["CONSTANT_1", "CONSTANT_2"]

    data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
    evaluation = data_dir.get_symbol([], ["CONSTANT_1"]).eval
    assert evaluation
    assert isinstance(evaluation.symbol, weakref.ref)
    assert evaluation.symbol() #Symbol of variable in constants.py
    assert evaluation.instance == True
    var_symbol = evaluation.symbol()
    assert var_symbol.type == SymType.VARIABLE
    assert var_symbol.name == "CONSTANT_1"
    
    base_test_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"])
    evaluation = base_test_file.get_symbol([], ["BaseOtherName"]).eval
    assert evaluation.symbol
    assert evaluation.symbol()
    assert evaluation.symbol().type == SymType.CLASS
    assert evaluation.symbol().name == "BaseTestModel"
    assert evaluation.instance == False

    evaluation = base_test_file.get_symbol([], ["baseInstance1"]).eval
    assert evaluation.symbol
    assert evaluation.symbol()
    assert evaluation.symbol().type == SymType.CLASS
    assert evaluation.symbol().name == "BaseTestModel"
    assert evaluation.instance == True

    evaluation = base_test_file.get_symbol([], ["baseInstance2"]).eval
    assert evaluation.symbol
    assert evaluation.symbol()
    assert evaluation.symbol().type == SymType.CLASS
    assert evaluation.symbol().name == "BaseTestModel"
    assert evaluation.instance == True

    evaluation = base_test_file.get_symbol([], ["ref_funcBase1"]).eval
    assert evaluation.symbol
    assert evaluation.symbol()
    assert evaluation.symbol().type == SymType.FUNCTION
    assert evaluation.symbol().name == "get_test_int"
    assert evaluation.instance == False

    evaluation = base_test_file.get_symbol([], ["ref_funcBase2"]).eval
    assert evaluation.symbol
    assert evaluation.symbol()
    assert evaluation.symbol().type == SymType.FUNCTION
    assert evaluation.symbol().name == "get_test_int"
    assert evaluation.instance == False
    
    evaluation = base_test_file.get_symbol([], ["return_funcBase2"]).eval
    #the return evaluation of a function is not really 100% accurate. Let's at least test that the function is not returned
    if evaluation.symbol and evaluation.symbol():
        assert evaluation.symbol().type != SymType.FUNCTION
        assert evaluation.symbol().name != "get_test_int"


def test_base_class():
    test_class = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"], ["BaseTestModel"])
    model_symbol = Odoo.get().symbols.get_symbol(["odoo", "models"], ["Model"])
    abstract_model = Odoo.get().symbols.get_symbol(["odoo", "models"], ["AbstractModel"])
    base_model = Odoo.get().symbols.get_symbol(["odoo", "models"], ["BaseModel"])
    assert test_class and test_class.type == SymType.CLASS and test_class.classData
    assert model_symbol and model_symbol.type == SymType.CLASS and test_class.classData
    assert abstract_model and abstract_model.type == SymType.VARIABLE
    assert base_model and base_model.type == SymType.CLASS and test_class.classData
    assert model_symbol in test_class.classData.bases
    assert abstract_model not in model_symbol.classData.bases
    assert base_model in model_symbol.classData.bases


def test_imports_dynamic():
    file_uri = get_uri(['data', 'addons', 'module_1', 'constants', 'data', 'constants.py'])
    
    server.workspace.get_document = Mock(return_value=Document(
        uri=file_uri,
        source="""
__all__ = ["CONSTANT_1"]

CONSTANT_1 = 1
CONSTANT_3 = 3"""
    ))
    params = DidChangeTextDocumentParams(
        text_document = VersionedTextDocumentIdentifier(
            version = 2,
            uri=file_uri
        ),
        content_changes = []
    )
    _did_change_after_delay(server, params, 0) #call deferred func
    base_test_models = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"])
    assert "CONSTANT_1" in base_test_models.symbols
    assert "CONSTANT_2" in base_test_models.symbols, "even if CONSTANT_2 is not in file anymore, the symbol should still exist"
    assert not "CONSTANT_3" in base_test_models.symbols
    constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
    assert "CONSTANT_1" in constants_dir.symbols
    assert "CONSTANT_2" in constants_dir.symbols
    assert not "CONSTANT_3" in constants_dir.symbols
    constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
    assert "CONSTANT_1" in constants_data_dir.symbols
    assert "CONSTANT_2" in constants_data_dir.symbols
    assert not search_in_local(constants_data_dir, "CONSTANT_2")
    assert not "CONSTANT_3" in constants_data_dir.symbols
    constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "constants"])
    assert "CONSTANT_1" in constants_data_file.symbols
    assert not "CONSTANT_2" in constants_data_file.symbols
    assert "CONSTANT_3" in constants_data_file.symbols

def test_rename():
    old_uri = get_uri(["data", "addons", "module_1", "constants", "data", "constants.py"])
    new_uri = get_uri(["data", "addons", "module_1", "constants", "data", "variables.py"])
    file = FileRename(old_uri, new_uri)
    params = RenameFilesParams([file])
    did_rename_files(server, params)
    constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
    assert "CONSTANT_1" in constants_dir.symbols
    assert "CONSTANT_2" in constants_dir.symbols
    assert not "CONSTANT_3" in constants_dir.symbols
    constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
    assert "CONSTANT_1" in constants_data_dir.symbols
    evaluation1 = constants_data_dir.symbols["CONSTANT_1"].eval
    assert not evaluation1.symbol()
    assert "CONSTANT_2" in constants_data_dir.symbols
    assert not search_in_local(constants_data_dir, "CONSTANT_2")
    assert not "CONSTANT_3" in constants_data_dir.symbols
    assert "variables" not in constants_data_dir.moduleSymbols
    constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "constants"])
    assert constants_data_file == None
    constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "variables"])
    assert constants_data_file == None #the file is not imported, so should not be available

    def test_missing_symbol_resolve():
        file_uri = get_uri(['data', 'addons', 'module_1', 'constants', 'data', '__init__.py'])
        
        server.workspace.get_document = Mock(return_value=Document(
            uri=file_uri,
            source="""
from .variables import *

CONSTANT_2 = 22"""
        ))
        params = DidChangeTextDocumentParams(
            text_document = VersionedTextDocumentIdentifier(
                version = 2,
                uri=file_uri
            ),
            content_changes = []
        )
        _did_change_after_delay(server, params, 0) #call deferred func
        constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
        assert "CONSTANT_1" in constants_dir.symbols
        assert "CONSTANT_2" in constants_dir.symbols
        assert not "CONSTANT_3" in constants_dir.symbols
        constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
        assert "CONSTANT_1" in constants_data_dir.symbols
        assert "CONSTANT_2" in constants_data_dir.symbols
        assert not search_in_local(constants_data_dir, "CONSTANT_2")
        assert not "CONSTANT_3" in constants_data_dir.symbols
        variables_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "variables"])
        assert "CONSTANT_1" in variables_data_file.symbols
        assert not "CONSTANT_2" in variables_data_file.symbols
        assert "CONSTANT_3" in variables_data_file.symbols