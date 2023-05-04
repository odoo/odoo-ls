import asyncio
import io
import json
import threading
import time
import weakref

import pytest
from lsprotocol.types import (CompletionParams, TextDocumentIdentifier, Position)
from pygls.server import StdOutTransportAdapter
from pygls.workspace import Document, Workspace

from ...server import (
    completions,
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

def test_autocomplete():
    file_uri = get_uri(['data', 'addons', 'module_1', 'constants', 'data', 'constants.py'])
    
    server.workspace.get_document = Mock(return_value=Document(
        uri=file_uri,
        source='''
from odoo import api, fields, models, _, tools


class TestModel(odoo.Models):
    
    _inherit = "'''
    ))
    params = CompletionParams(
        text_document=TextDocumentIdentifier(
            uri=file_uri,
        ),
        position=Position(
            line=7,
            character=15,
        )
    )

    items = completions(server, params)