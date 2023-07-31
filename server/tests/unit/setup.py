import os
import pathlib
from concurrent.futures import Future
from mock import Mock
from pygls.workspace import Document, Workspace, WorkspaceFolder

from ...server import (
    OdooLanguageServer
)
from server.core.fileMgr import FileMgr
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

ODOO_COMMUNITY_PATH = '/home/odoo/Documents/odoo-servers/server_d/odoo'
if os.name == "nt":
    ODOO_COMMUNITY_PATH = 'E:\Mes Documents\odoo\community'

# Prepare DATA

test_addons_path = pathlib.Path(__file__).parent.parent.resolve()
test_addons_path = os.path.join(test_addons_path, 'data', 'addons')

server = OdooLanguageServer()
server.publish_diagnostics = Mock()
server.show_message = Mock()
server.show_message_log = Mock()
server.lsp.workspace = Workspace('', None,
                                workspace_folders=[WorkspaceFolder(test_addons_path, "addons"), WorkspaceFolder(ODOO_COMMUNITY_PATH, "odoo")])
server.lsp._send_only_body = True

class MockConfig(object):
    pass

config = MockConfig()
config.id = 1
config.name = "Used configuration"
config.odooPath = ODOO_COMMUNITY_PATH
config.addons = [test_addons_path]

config_result = Future()
config_result.set_result(config)

def  request_side_effect(*args, **kwargs):
    if args[0] == 'Odoo/getConfiguration':
        return config_result

# There is a possibility this might mess with other send_request calls
# Consider this a temporary fix - sode
server.lsp.send_request = Mock(side_effect=request_side_effect)

def get_uri(path):
    #return an uri from the "tests" level with a path like ["data", "module1"]
    file_uri = pathlib.Path(__file__).parent.parent.resolve()
    file_uri = os.path.join(file_uri, *path)
    return FileMgr.pathname2uri(file_uri)
