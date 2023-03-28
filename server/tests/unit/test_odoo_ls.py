import asyncio
import io
import json
import threading
import time

import pytest
from lsprotocol.types import (DidChangeTextDocumentParams, VersionedTextDocumentIdentifier)
from pygls.server import StdOutTransportAdapter
from pygls.workspace import Document, Workspace

from ...server import (
    did_change
)
from .setup import *
from ...odoo import Odoo

"""
XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

To run / setup tests, please see setup.py

XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
"""

Odoo.get(server)

def _reset_mocks(stdin=None, stdout=None):
    stdin = stdin or io.StringIO()
    stdout = stdout or io.StringIO()

    server.lsp.transport = StdOutTransportAdapter(stdin, stdout)
    server.publish_diagnostics.reset_mock()
    server.show_message.reset_mock()
    server.show_message_log.reset_mock()

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
    assert "CONSTANT_2" in constants_data_dir.symbols
    assert not "CONSTANT_3" in constants_data_dir.symbols

def test_load_classes():
    base_class = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "base_test_models"], ["BaseTestModel"])
    assert base_class, "BaseTestModel has not been loaded"
    assert base_class.name == "BaseTestModel"
    assert "test_int" in base_class.symbols
    assert "get_test_int" in base_class.symbols
    assert "get_constant" in base_class.symbols


def test_imports_dynamic():
    server.workspace.get_document = Mock(return_value=Document(
        uri="file://server/tests/data/addons/module_1/constants/data/constants.py",
        source="""
__all__ = ["CONSTANT_1"]

CONSTANT_1 = 1
CONSTANT_3 = 3"""
    ))
    params = DidChangeTextDocumentParams(
        text_document = VersionedTextDocumentIdentifier(
            version = 2,
            uri="will be mock by workspace.get_document"
        ),
        content_changes = []
    )
    did_change(server, params)