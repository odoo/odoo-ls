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
from .odoo import Odoo
from server.pythonArchBuilder import PythonArchBuilder
from server.pythonUtils import PythonUtils
from server.fileMgr import *
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

def _validate(ls, params):
    if Odoo.isLoading:
        return
    ls.show_message_log('Validating odoo...')

    text_doc = ls.workspace.get_document(params.text_document.uri)

    source = text_doc.source
    diagnostics = _validate_json(source) if source else []

    #ls.publish_diagnostics(text_doc.uri, diagnostics)

@odoo_server.feature(TEXT_DOCUMENT_COMPLETION, CompletionOptions(trigger_characters=[',']))
def completions(params: Optional[CompletionParams] = None) -> CompletionList:
    """Returns completion items."""
    if Odoo.isLoading:
        return None
    print("completion")
    return CompletionList(
        is_incomplete=False,
        items=[
            CompletionItem(label='"'),
            CompletionItem(label='['),
            CompletionItem(label=']'),
            CompletionItem(label='{'),
            CompletionItem(label='}'),
        ]
    )

@odoo_server.feature(TEXT_DOCUMENT_HOVER)
def hover(params: TextDocumentPositionParams):
    if Odoo.isLoading:
        return None
    final_path = urllib.parse.urlparse(urllib.parse.unquote(params.text_document.uri)).path
    final_path = urllib.request.url2pathname(final_path)
    #TODO find better than this small hack for windows (get disk letter in capital)
    if os.name == "nt":
        final_path = final_path[0].capitalize() + final_path[1:]
    file_symbol = Odoo.get().get_file_symbol(final_path)
    if file_symbol and params.text_document.uri[-3:] == ".py":
        symbol = PythonUtils.getSymbol(file_symbol, params.position.line + 1, params.position.character + 1)
    hover = Hover(symbol and symbol.name)
    return hover

@odoo_server.feature(TEXT_DOCUMENT_DEFINITION)
def definition(params: TextDocumentPositionParams):
    """Returns the location of a symbol definition"""
    if Odoo.isLoading:
        return None
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
    if Odoo.isLoading:
        return
    with odoo_server.id_lock:
        odoo_server.id += 1
        id = odoo_server.id
    #As we don't want to validate on each change immediately, we wait a bit before rebuilding.
    #The id ensure we do the rebuild only if this is the last change.
    threading.Timer(2.0, _did_change_after_delay, [ls, params, id]).start()

@odoo_server.feature(WORKSPACE_DID_RENAME_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
def did_rename_files(ls, params):
    """Workspace did rename files notification."""
    if Odoo.isLoading:
        return
    for f in params.files:
        final_path = urllib.parse.urlparse(urllib.parse.unquote(f.old_uri)).path
        final_path = urllib.request.url2pathname(final_path)
        #TODO find better than this small hack for windows (get disk letter in capital)
        if os.name == "nt":
            final_path = final_path[0].capitalize() + final_path[1:]
        Odoo.get(ls).file_rename(ls, final_path, f.new_uri)

@odoo_server.feature(TEXT_DOCUMENT_DID_CLOSE)
def did_close(server: OdooLanguageServer, params: DidCloseTextDocumentParams):
    """Text document did close notification."""
    pass

@odoo_server.thread()
@odoo_server.feature(TEXT_DOCUMENT_DID_OPEN)
def did_open(ls, params: DidOpenTextDocumentParams):
    """Text document did open notification."""
    Odoo.get(ls)
