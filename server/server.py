############################################################################
# Copyright(c) Open Law Library. All rights reserved.                      #
# See ThirdPartyNotices.txt in the project root for additional notices.    #
#                                                                          #
# Licensed under the Apache License, Version 2.0 (the "License")           #
# you may not use this file except in compliance with the License.         #
# You may obtain a copy of the License at                                  #
#                                                                          #
#     http: // www.apache.org/licenses/LICENSE-2.0                         #
#                                                                          #
# Unless required by applicable law or agreed to in writing, software      #
# distributed under the License is distributed on an "AS IS" BASIS,        #
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. #
# See the License for the specific language governing permissions and      #
# limitations under the License.                                           #
############################################################################
import asyncio
import json
import os
import re
import time
import uuid
import threading
from json import JSONDecodeError
from typing import Optional
from .core.odoo import Odoo
from server.core.pythonArchBuilder import PythonArchBuilder
from server.pythonUtils import PythonUtils
from server.core.fileMgr import *
from server.features.autocomplete import AutoCompleteFeature
from server.features.hover import HoverFeature
import urllib.parse
import urllib.request

from lsprotocol.types import *
from lsprotocol.types import (CompletionItem, CompletionList, CompletionOptions,
                             CompletionParams, ConfigurationItem,
                             ConfigurationParams, Diagnostic,
                             DidChangeTextDocumentParams,
                             DidCloseTextDocumentParams,
                             DidOpenTextDocumentParams, MessageType, Position,
                             Range, Registration, RegistrationParams,
                             SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
                             Unregistration, UnregistrationParams,
                             TextDocumentPositionParams, Location, Hover)
from lsprotocol.types import (WorkDoneProgressBegin,
                                WorkDoneProgressEnd,
                                WorkDoneProgressReport)
from pygls.server import LanguageServer
from .constants import *

COUNT_DOWN_START_IN_SECONDS = 10
COUNT_DOWN_SLEEP_IN_SECONDS = 1


class OdooLanguageServer(LanguageServer):

    def __init__(self):
        print("Starting Odoo Language server")
        self.id_lock = threading.Lock()
        self.id = 0
        super().__init__(name=EXTENSION_NAME, version=EXTENSION_VERSION)


odoo_server = OdooLanguageServer()

def get_path_file(uri):
    path = urllib.parse.urlparse(urllib.parse.unquote(uri)).path
    path = urllib.request.url2pathname(path)
    #TODO find better than this small hack for windows (get disk letter in capital)
    if os.name == "nt":
        path = path[0].capitalize() + path[1:]
    return path

@odoo_server.feature(TEXT_DOCUMENT_COMPLETION, CompletionOptions(trigger_characters=[',', '.', '"', "'"]))
def completions(ls, params: Optional[CompletionParams] = None) -> CompletionList:
    """Returns completion items."""
    if not params:
        print("no params")
        return None
    print("completion")
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = get_path_file(params.text_document.uri)
    with Odoo.get().acquire_read():
        return AutoCompleteFeature.autocomplete(path, content, params.position.line, params.position.character)

@odoo_server.feature(TEXT_DOCUMENT_HOVER)
def hover(ls, params: TextDocumentPositionParams):
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = get_path_file(params.text_document.uri)
    with Odoo.get().acquire_read():
        file_symbol = Odoo.get().get_file_symbol(path)
        if file_symbol and params.text_document.uri[-3:] == ".py":
            #Force the parsoTree to be loaded by giving file content and opened==True
            parsoTree = FileMgr.getFileInfo(file_symbol.paths[0], content, opened=True)["parsoTree"]
            symbol, range, context = HoverFeature.getSymbol(file_symbol, parsoTree, params.position.line + 1, params.position.character + 1)
            return HoverFeature.get_Hover(symbol, range, context)
    return None

@odoo_server.feature(TEXT_DOCUMENT_DEFINITION)
def definition(params: TextDocumentPositionParams):
    """Returns the location of a symbol definition"""
    final_path = urllib.parse.urlparse(urllib.parse.unquote(params.text_document.uri)).path
    final_path = urllib.request.url2pathname(final_path)
    #TODO find better than this small hack for windows (get disk letter in capital)
    if os.name == "nt":
        final_path = final_path[0].capitalize() + final_path[1:]
    file_symbol = Odoo.get().get_file_symbol(final_path)
    if file_symbol and params.text_document.uri[-3:] == ".py":
        symbol = PythonUtils.getSymbol(file_symbol, params.position.line + 1, params.position.character + 1)
    if symbol:
        #TODO paths?
        a = Location(uri=FileMgr.pathname2uri(symbol.paths[0]), range=Range(start=Position(line=symbol.startLine-1, character=0), end=Position(line=symbol.endLine-1, character=0)))
        return [a]
    return []

@odoo_server.thread()
def _did_change_after_delay(ls, params: DidChangeTextDocumentParams, reg_id):
    id = 0
    with odoo_server.id_lock:
        id = odoo_server.id
        if id != reg_id:
            return
    text_doc = ls.workspace.get_document(params.text_document.uri)
    source = text_doc.source
    final_path = urllib.parse.urlparse(urllib.parse.unquote(params.text_document.uri)).path
    final_path = urllib.request.url2pathname(final_path)
    #TODO find better than this small hack for windows (get disk letter in capital)
    if os.name == "nt":
        final_path = final_path[0].capitalize() + final_path[1:]
    Odoo.get(ls).file_change(ls, final_path, source, params.text_document.version)
    print("done")

@odoo_server.feature(TEXT_DOCUMENT_DID_CHANGE)
def did_change(ls, params: DidChangeTextDocumentParams):
    """Text document did change notification."""
    #TODO A change should probably not be discarded even if Odoo is loading, as we maybe want to rebuild these changes
    with odoo_server.id_lock:
        odoo_server.id += 1
        id = odoo_server.id
    #As we don't want to validate on each change immediately, we wait a bit before rebuilding.
    #The id ensure we do the rebuild only if this is the last change.
    threading.Timer(1.0, _did_change_after_delay, [ls, params, id]).start()

@odoo_server.feature(WORKSPACE_DID_RENAME_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
def did_rename_files(ls, params):
    """Workspace did rename files notification."""
    #TODO A change should probably not be discarded even if Odoo is loading, as we maybe want to rebuild these changes
    for f in params.files:
        old_path = urllib.parse.urlparse(urllib.parse.unquote(f.old_uri)).path
        old_path = urllib.request.url2pathname(old_path)
        new_path = urllib.parse.urlparse(urllib.parse.unquote(f.new_uri)).path
        new_path = urllib.request.url2pathname(new_path)
        #TODO find better than this small hack for windows (get disk letter in capital)
        if os.name == "nt":
            old_path = old_path[0].capitalize() + old_path[1:]
            new_path = new_path[0].capitalize() + new_path[1:]
        Odoo.get(ls).file_rename(ls, old_path, new_path)

@odoo_server.feature(TEXT_DOCUMENT_DID_CLOSE)
def did_close(server: OdooLanguageServer, params: DidCloseTextDocumentParams):
    """Text document did close notification."""
    path = get_path_file(params.text_document.uri)
    FileMgr.removeParsoTree(path)

@odoo_server.feature(TEXT_DOCUMENT_DID_OPEN)
def did_open(ls, params: DidOpenTextDocumentParams):
    """Text document did open notification."""
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = get_path_file(params.text_document.uri)
    FileMgr.getFileInfo(path, content, params.text_document.version, opened = True)


@odoo_server.thread()
@odoo_server.feature("Odoo/clientReady")
def client_ready(ls, params=None):
    Odoo.get(ls)
