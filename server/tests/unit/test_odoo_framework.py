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

from ...controller import (
    did_rename_files
)
from ...update_event_queue import EditEvent, UpdateEvent, UpdateEventType
from .setup import *
from ...core.odoo import Odoo
from ...core.symbol import Symbol, ClassSymbol
from ...constants import *
from ...references import RegisteredRef
from ...python_utils import PythonUtils
from ...features.parso_utils import ParsoUtils

"""
XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

To run / setup tests, please see setup.py

XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
"""

Odoo.import_odoo_addons = False
Odoo.initialize(server)

def test_odoo_fields():
    cl = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"], ["model_name_inherit_comb_name"])
    assert cl
    cl_func = cl.get_symbol([], ["a_random_func"])
    assert cl_func
    models_uri = get_uri(['data', 'addons', 'module_1', 'models', 'models.py'])
    text_doc = server.workspace.get_document(models_uri)
    assert text_doc
    content = text_doc.source
    assert content
    parsoTree = Odoo.get().grammar.parse(content, error_recovery=True, cache = False)
    assert parsoTree
    ###### int ######
    cl_int = cl.get_symbol([], ["an_int"])
    assert cl_int
    assert cl_int.eval
    assert cl_int.eval.get_symbol() and cl_int.eval.get_symbol().name == "Integer"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+1, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Integer"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Integer"
    assert symbol_ancestors and symbol_ancestors.name == "int"
    ###### Bool ######
    cl_bool = cl.get_symbol([], ["a_bool"])
    assert cl_bool
    assert cl_bool.eval
    assert cl_bool.eval.get_symbol() and cl_bool.eval.get_symbol().name == "Boolean"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+2, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Boolean"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Boolean"
    assert symbol_ancestors and symbol_ancestors.name == "bool"
    ###### Char ######
    cl_char = cl.get_symbol([], ["a_char"])
    assert cl_char
    assert cl_char.eval
    assert cl_char.eval.get_symbol() and cl_char.eval.get_symbol().name == "Char"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+3, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Char"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Char"
    assert symbol_ancestors and symbol_ancestors.name == "str"
    ###### Text ######
    cl_text = cl.get_symbol([], ["a_text"])
    assert cl_text
    assert cl_text.eval
    assert cl_text.eval.get_symbol() and cl_text.eval.get_symbol().name == "Text"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+4, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Text"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Text"
    assert symbol_ancestors and symbol_ancestors.name == "str"
    ###### Float ######
    cl_float = cl.get_symbol([], ["a_float"])
    assert cl_float
    assert cl_float.eval
    assert cl_float.eval.get_symbol() and cl_float.eval.get_symbol().name == "Float"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+5, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Float"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Float"
    assert symbol_ancestors and symbol_ancestors.name == "float"
    ###### Date ######
    cl_date = cl.get_symbol([], ["a_date"])
    assert cl_date
    assert cl_date.eval
    assert cl_date.eval.get_symbol() and cl_date.eval.get_symbol().name == "Date"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+6, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Date"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Date"
    assert symbol_ancestors and symbol_ancestors.name == "date"
    ###### DateTime ######
    cl_datetime = cl.get_symbol([], ["a_datetime"])
    assert cl_datetime
    assert cl_datetime.eval
    assert cl_datetime.eval.get_symbol() and cl_datetime.eval.get_symbol().name == "Datetime"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+7, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Datetime"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Datetime"
    assert symbol_ancestors and symbol_ancestors.name == "datetime"
    ###### Selection ######
    cl_selection = cl.get_symbol([], ["a_selection"])
    assert cl_selection
    assert cl_selection.eval
    assert cl_selection.eval.get_symbol() and cl_selection.eval.get_symbol().name == "Selection"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+8, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Selection"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Selection"
    assert symbol_ancestors and symbol_ancestors.name == "str"

    ###### Relationnals ######

    cl_inherits = Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1", "models", "models"], ["model_inherits"])
    assert cl_inherits
    cl_func = cl_inherits.get_symbol([], ["a_random_func"])
    assert cl_func

    ###### One2many ######
    cl_field_name = cl_inherits.get_symbol([], ["field_m_name_id"])
    assert cl_field_name
    assert cl_field_name.eval
    assert cl_field_name.eval.get_symbol() and cl_field_name.eval.get_symbol().name == "Many2one"
    element = parsoTree.get_leaf_for_position((cl_func.start_pos[0]+1, 17), include_prefixes=True)
    expr = ParsoUtils.get_previous_leafs_expr(element)
    sym, symbol_ancestors, factory, context = ParsoUtils.evaluate_expr(expr + [element], cl_func)
    assert factory and factory.name == "Many2one"
    assert sym
    assert symbol_ancestors
    assert context
    assert sym[0].eval.get_symbol() and sym[0].eval.get_symbol().name == "Many2one"
    assert symbol_ancestors and symbol_ancestors.name == "model_name"