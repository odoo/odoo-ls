from lsprotocol.types import (CompletionParams, TextDocumentIdentifier, Position)
from pygls.workspace import Document

from ...server import (
    completions,
)
from .setup import *
from ...core.odoo import Odoo
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
            line=6,
            character=15,
        )
    )

    items = completions(server, params)
    print(items)