import os
import pathlib
from concurrent.futures import Future
from mock import Mock
from pygls.workspace import Document, Workspace

from ...server import (
    OdooLanguageServer
)
from ...fileMgr import FileMgr
"""
To run tests:

pip install pytest, mock, pytest-asyncio, pytest-dependency
cd server/tests/unit
set up the next constants to match your local configuration, then
pytest test_setup.py -- test that your setup is correct and that OdooLS is starting correctly
pytest test_odoo_ls.py -- test the different OdooLS functionalities
pytest test_odoo_requests.py -- test the different OdooLS requests

add -s if you want to see the logs from OdooLS
"""

# SETUP CONSTANTS

ODOO_COMMUNITY_PATH = '/home/odoo/Documents/odoo-servers/test_odoo/odoo'
if os.name == "nt":
    ODOO_COMMUNITY_PATH = 'E:\Mes Documents\odoo\community'

# Prepare DATA

test_addons_path = pathlib.Path(__file__).parent.parent.resolve()
test_addons_path = os.path.join(test_addons_path, 'data', 'addons')

server = OdooLanguageServer()
server.publish_diagnostics = Mock()
server.show_message = Mock()
server.show_message_log = Mock()
server.lsp.workspace = Workspace('', None)
server.lsp._send_only_body = True

config_result = Future()
config_result.set_result([{
    'userDefinedConfigurations': {
        '0': {'id': 0, 'name': 'Configuration 0', 'odooPath': 'path/to/odoo', 'addons': []}, 
        '1': {'id': 1, 'name': 'Used configuration', 'odooPath': ODOO_COMMUNITY_PATH, 'addons': [test_addons_path]}, 
        '2': {'id': 2, 'name': 'Configuration 2', 'odooPath': 'path/to/odoo', 'addons': []}, 
        '4': {'id': 4, 'name': 'Skipped Id', 'odooPath': 'path/to/odoo', 'addons': []}
    }, 
    'selectedConfigurations': 1, 
    'trace': {
        'server': 'off'}
    }]
)

server.lsp.get_configuration = Mock(return_value=config_result)

def get_uri(path):
    #return an uri from the "tests" level with a path like ["data", "module1"]
    file_uri = pathlib.Path(__file__).parent.parent.resolve()
    file_uri = os.path.join(file_uri, *path)
    return FileMgr.pathname2uri(file_uri)