import asyncio
import io
import json
import threading
import time

import pytest
from unittest.mock import patch, mock_open
from lsprotocol.types import (DidChangeTextDocumentParams, VersionedTextDocumentIdentifier, RenameFilesParams, FileRename)
from pygls.server import StdOutTransportAdapter
from pygls.workspace import Document, Workspace

from ...server import (
    _did_change_after_delay,
    did_rename_files
)
from .setup import *
from server.core.odoo import Odoo
from server.core.symbol import Symbol
from server.constants import *
from server.references import RegisteredRef

"""
XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

To run / setup tests, please see setup.py

XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
"""

Odoo.import_odoo_addons = False
Odoo.get(server)

def search_in_local(symbol, name):
    found_in_local = False
    for sym in symbol.localSymbols:
        if sym.name == name:
            found_in_local = True
            break
    return found_in_local

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
    assert not "CONSTANT_3" in constants_dir.symbols, "CONSTANT_3 should not be loaded, as __all__ variable should prevent import in constants.py"
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
    assert evaluation and evaluation.get_symbol()
    assert isinstance(evaluation._symbol, RegisteredRef)
    assert evaluation.get_symbol().type == SymType.PRIMITIVE
    assert evaluation.instance == True
    evaluation = constants_data_file.get_symbol([], ["__all__"]).eval
    assert evaluation and evaluation.get_symbol()
    assert isinstance(evaluation._symbol, RegisteredRef)
    assert evaluation.get_symbol().type == SymType.PRIMITIVE
    assert evaluation.instance == True
    assert evaluation.get_symbol().name == "list"
    assert evaluation.get_symbol().eval.value == ["CONSTANT_1", "CONSTANT_2"]

    data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
    evaluation = data_dir.get_symbol([], ["CONSTANT_1"]).eval
    assert evaluation
    assert isinstance(evaluation._symbol, RegisteredRef)
    assert evaluation.get_symbol() #Symbol of variable in constants.py
    assert evaluation.instance == True
    var_symbol = evaluation.get_symbol()
    assert var_symbol.type == SymType.VARIABLE
    assert var_symbol.name == "CONSTANT_1"

    base_test_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"])
    evaluation = base_test_file.get_symbol([], ["BaseOtherName"]).eval
    assert evaluation._symbol
    assert evaluation.get_symbol()
    assert evaluation.get_symbol().type == SymType.CLASS
    assert evaluation.get_symbol().name == "BaseTestModel"
    assert evaluation.instance == False

    evaluation = base_test_file.get_symbol([], ["baseInstance1"]).eval
    assert evaluation._symbol
    assert evaluation.get_symbol()
    assert evaluation.get_symbol().type == SymType.CLASS
    assert evaluation.get_symbol().name == "BaseTestModel"
    assert evaluation.instance == True

    evaluation = base_test_file.get_symbol([], ["baseInstance2"]).eval
    assert evaluation._symbol
    assert evaluation.get_symbol()
    assert evaluation.get_symbol().type == SymType.CLASS
    assert evaluation.get_symbol().name == "BaseTestModel"
    assert evaluation.instance == True

    evaluation = base_test_file.get_symbol([], ["ref_funcBase1"]).eval
    assert evaluation._symbol
    assert evaluation.get_symbol()
    assert evaluation.get_symbol().type == SymType.FUNCTION
    assert evaluation.get_symbol().name == "get_test_int"
    assert evaluation.instance == False

    evaluation = base_test_file.get_symbol([], ["ref_funcBase2"]).eval
    assert evaluation._symbol
    assert evaluation.get_symbol()
    assert evaluation.get_symbol().type == SymType.FUNCTION
    assert evaluation.get_symbol().name == "get_test_int"
    assert evaluation.instance == False

    evaluation = base_test_file.get_symbol([], ["return_funcBase2"]).eval
    #the return evaluation of a function is not really 100% accurate. Let's at least test that the function is not returned
    if evaluation._symbol and evaluation.get_symbol():
        assert evaluation.get_symbol().type != SymType.FUNCTION
        assert evaluation.get_symbol().name != "get_test_int"


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


def test_model_name_inherit():
    model_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"])
    model_name = model_file.get_symbol([], ["model_name"])
    assert model_name and model_name.modelData
    assert model_name.modelData.name == "pygls.tests.m_name"
    assert model_name.modelData.inherit == ["base"]
    model_name_inherit = model_file.get_symbol([], ["model_name_inherit"])
    assert model_name_inherit and model_name_inherit.modelData
    assert model_name_inherit.modelData.name == "pygls.tests.m_name"
    assert model_name_inherit.modelData.inherit == ["pygls.tests.m_name", "base"]
    model_name_inherit_no_name = model_file.get_symbol([], ["model_name_inherit_no_name"])
    assert model_name_inherit_no_name and model_name_inherit_no_name.modelData
    assert model_name_inherit_no_name.modelData.name == "pygls.tests.m_name"
    assert model_name_inherit_no_name.modelData.inherit == ["pygls.tests.m_name", "base"]
    model_name_inherit_diff_name = model_file.get_symbol([], ["model_name_inherit_diff_name"])
    assert model_name_inherit_diff_name and model_name_inherit_diff_name.modelData
    assert model_name_inherit_diff_name.modelData.name == "pygls.tests.m_diff_name"
    assert model_name_inherit_diff_name.modelData.inherit == ["pygls.tests.m_name", "base"]
    model_name_2 = model_file.get_symbol([], ["model_name_2"])
    assert model_name_2 and model_name_2.modelData
    assert model_name_2.modelData.name == "pygls.tests.m_name_2"
    assert model_name_2.modelData.inherit == ["base"]
    model_name_inherit_comb_name = model_file.get_symbol([], ["model_name_inherit_comb_name"])
    assert model_name_inherit_comb_name and model_name_inherit_comb_name.modelData
    assert model_name_inherit_comb_name.modelData.name == "pygls.tests.m_comb_name"
    assert model_name_inherit_comb_name.modelData.inherit == ["pygls.tests.m_name", "pygls.tests.m_name_2", "base"]
    model_no_name = model_file.get_symbol([], ["model_no_name"])
    assert model_no_name and model_no_name.modelData
    assert model_no_name.modelData.name == "model_no_name"
    model_no_register = model_file.get_symbol([], ["model_no_register"])
    assert model_no_register and model_no_register.modelData
    assert model_no_register.modelData.name == ""
    assert model_no_register.modelData.inherit == []
    model_no_register = model_file.get_symbol([], ["model_register"])
    assert model_no_register and model_no_register.modelData
    assert model_no_register.modelData.name == "pygls.tests.m_no_register"
    assert model_no_register.modelData.inherit == ['base']
    model_no_register_inherit = model_file.get_symbol([], ["model_no_register_inherit"])
    assert model_no_register_inherit and model_no_register_inherit.modelData
    assert model_no_register_inherit.modelData.name == "pygls.tests.m_no_register"
    assert model_no_register_inherit.modelData.inherit == ["pygls.tests.m_no_register", "base"]
    model_inherits = model_file.get_symbol([], ["model_inherits"])
    assert model_inherits and model_inherits.modelData
    assert model_inherits.modelData.name == "pygls.tests.m_inherits"
    assert model_inherits.modelData.inherits == {"pygls.tests.m_name": "field_m_name_id"}

def test_magic_fields():
    model_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"])
    if Odoo.instance.version_major == 14:
        model_model = model_file.get_symbol([], ["model_model"])
        assert model_model and model_model.modelData
        assert model_model and model_model.modelData.auto == True
        model_model = model_file.get_symbol([], ["model_transient"])
        assert model_model and model_model.modelData
        assert model_model and model_model.modelData.auto == True
        model_model = model_file.get_symbol([], ["model_abstract"])
        assert model_model and model_model.modelData
        assert model_model and model_model.modelData.auto == False
        model_model = model_file.get_symbol([], ["model_name"])
        assert model_model and model_model.modelData
        assert model_model and model_model.modelData.auto == False
        model_model = model_file.get_symbol([], ["model_name_inh_python"])
        assert model_model and model_model.modelData
        assert model_model and model_model.modelData.auto == False
    elif Odoo.instance.version_major == 15:
        assert True
    elif Odoo.instance.version_major == 16:
        assert True
    else:
        assert False

@pytest.mark.dependency()
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

@pytest.mark.dependency(depends=["test_imports_dynamic"])
def test_rename():
    print("RENAME TEST")
    old_uri_mock = pathlib.Path(__file__).parent.parent.resolve()
    old_uri_mock = os.path.join(old_uri_mock, "data", "addons", "module_1", "constants", "data", "constants.py")
    with open(old_uri_mock, "rb") as f:
        data = f.read() #mock old file content
        with patch("builtins.open", mock_open(read_data=data)) as mock_file:
            old_uri = get_uri(["data", "addons", "module_1", "constants", "data", "constants.py"])
            new_uri = get_uri(["data", "addons", "module_1", "constants", "data", "variables.py"])
            file = FileRename(old_uri, new_uri)
            params = RenameFilesParams([file])
            mock = Mock()
            normal_isfile = os.path.isfile
            def _validated_variables_file(*args, **kwargs):
                if "constants.py" in args[0]:
                    return False
                elif "variables.py" in args[0]:
                    return True
                else:
                    return normal_isfile(*args, **kwargs)
            def _validated_constants_file(*args, **kwargs):
                if "constants.py" in args[0]:
                    return True
                elif "variables.py" in args[0]:
                    return False
                else:
                    return normal_isfile(*args, **kwargs)
            mock.side_effect = _validated_variables_file
            os.path.isfile = mock # ensure that new file name is detected as valid
            did_rename_files(server, params)
            #A check that symbols are not imported anymore from old file
            constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
            assert "CONSTANT_1" not in constants_dir.symbols
            assert "CONSTANT_2" in constants_dir.symbols
            assert "CONSTANT_3" not in constants_dir.symbols
            constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
            assert "CONSTANT_1" not in constants_data_dir.symbols
            assert "CONSTANT_2" in constants_data_dir.symbols
            assert "CONSTANT_3" not in constants_data_dir.symbols
            assert not search_in_local(constants_data_dir, "CONSTANT_2")
            assert "variables" not in constants_data_dir.moduleSymbols
            constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "constants"])
            assert constants_data_file == None
            constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "variables"])
            assert constants_data_file == None #As the file is not imported by any file, it should not be available

            #B now change data/__init__.py to include the new file, and check that imports are resolved
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
            _did_change_after_delay(server, params, 0)

            var_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "variables"])
            assert var_data_file
            assert "CONSTANT_1" in var_data_file.symbols
            assert "CONSTANT_2" in var_data_file.symbols
            assert "CONSTANT_3" in var_data_file.symbols
            assert "__all__" in var_data_file.symbols
            constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
            assert "CONSTANT_1" in constants_dir.symbols
            assert "CONSTANT_2" in constants_dir.symbols
            assert "CONSTANT_3" not in constants_dir.symbols
            constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
            assert "CONSTANT_1" in constants_data_dir.symbols
            assert "CONSTANT_2" in constants_data_dir.symbols
            assert "CONSTANT_3" not in constants_data_dir.symbols
            assert search_in_local(constants_data_dir, "CONSTANT_2")
            assert "variables" in constants_data_dir.moduleSymbols
            constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "constants"])
            assert constants_data_file == None

            # C let's go back to old name, then rename again to variables, to see if everything resolve correctly
            os.path.isfile = Mock(return_value=False) #prevent disk access to old file
            old_uri = get_uri(["data", "addons", "module_1", "constants", "data", "variables.py"])
            new_uri = get_uri(["data", "addons", "module_1", "constants", "data", "constants.py"])
            file = FileRename(old_uri, new_uri)
            params = RenameFilesParams([file])
            mock.side_effect = _validated_constants_file
            did_rename_files(server, params)
            os.path.isfile = Mock(return_value=True) #prevent disk access to old file
            old_uri = get_uri(["data", "addons", "module_1", "constants", "data", "constants.py"])
            new_uri = get_uri(["data", "addons", "module_1", "constants", "data", "variables.py"])
            file = FileRename(old_uri, new_uri)
            params = RenameFilesParams([file])
            mock.side_effect = _validated_variables_file
            did_rename_files(server, params)

            var_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "variables"])
            assert var_data_file
            assert "CONSTANT_1" in var_data_file.symbols
            assert "CONSTANT_2" in var_data_file.symbols
            assert "CONSTANT_3" in var_data_file.symbols
            assert "__all__" in var_data_file.symbols
            constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
            assert "CONSTANT_1" in constants_dir.symbols
            assert "CONSTANT_2" in constants_dir.symbols
            assert "CONSTANT_3" not in constants_dir.symbols
            constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
            assert "CONSTANT_1" in constants_data_dir.symbols
            assert "CONSTANT_2" in constants_data_dir.symbols
            assert "CONSTANT_3" not in constants_data_dir.symbols
            assert search_in_local(constants_data_dir, "CONSTANT_2")
            assert "variables" in constants_data_dir.moduleSymbols
            constants_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "constants"])
            assert constants_data_file == None

            os.path.isfile = normal_isfile
            server.workspace.get_document.reset_mock()

def test_rename_inherit():
    model = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"], ["model_model"])
    assert model
    assert model.classData
    assert model.classData.bases
    file_uri = os.path.join(ODOO_COMMUNITY_PATH, 'odoo', 'models.py')
    source = ""
    with open(file_uri, 'r') as f:
        source = f.read()
    assert source
    source = source.replace("class Model", "class Model2")

    server.workspace.get_document = Mock(return_value=Document(
        uri=FileMgr.pathname2uri(file_uri),
        source=source
    ))
    params = DidChangeTextDocumentParams(
        text_document = VersionedTextDocumentIdentifier(
            version = 2,
            uri=FileMgr.pathname2uri(file_uri)
        ),
        content_changes = []
    )
    _did_change_after_delay(server, params, 0) #call deferred func
    model = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"], ["model_model"])
    assert model
    assert model.classData
    assert not model.classData.bases
    source = source.replace("class Model2", "class Model")

    server.workspace.get_document = Mock(return_value=Document(
        uri=FileMgr.pathname2uri(file_uri),
        source=source
    ))
    params = DidChangeTextDocumentParams(
        text_document = VersionedTextDocumentIdentifier(
            version = 3,
            uri=FileMgr.pathname2uri(file_uri)
        ),
        content_changes = []
    )
    _did_change_after_delay(server, params, 0) #call deferred func
    model = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"], ["model_model"])
    assert model
    assert model.classData
    assert model.classData.bases
    server.workspace.get_document.reset_mock()

def test_missing_symbol_resolve():
    #TODO write test
    pass
#     file_uri = get_uri(['data', 'addons', 'module_1', 'constants', 'data', '__init__.py'])

#     server.workspace.get_document = Mock(return_value=Document(
#         uri=file_uri,
#         source="""
# from .variables import *

# CONSTANT_2 = 22"""
#     ))
#     params = DidChangeTextDocumentParams(
#         text_document = VersionedTextDocumentIdentifier(
#             version = 2,
#             uri=file_uri
#         ),
#         content_changes = []
#     )
#     _did_change_after_delay(server, params, 0) #call deferred func
#     constants_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants"])
#     assert "CONSTANT_1" in constants_dir.symbols
#     assert "CONSTANT_2" in constants_dir.symbols
#     assert not "CONSTANT_3" in constants_dir.symbols
#     constants_data_dir = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data"])
#     assert "CONSTANT_1" not in constants_data_dir.symbols
#     assert "CONSTANT_2" in constants_data_dir.symbols
#     assert not search_in_local(constants_data_dir, "CONSTANT_2")
#     assert not "CONSTANT_3" in constants_data_dir.symbols
#     variables_data_file = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "constants", "data", "variables"])
#     assert "CONSTANT_1" in variables_data_file.symbols
#     assert not "CONSTANT_2" in variables_data_file.symbols
#     assert "CONSTANT_3" in variables_data_file.symbols

#     server.workspace.get_document.reset_mock()
