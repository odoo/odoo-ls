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
import sys
import threading
import traceback
from json import JSONDecodeError
from typing import Optional
from .core.odoo import Odoo
from server.core.pythonArchBuilder import PythonArchBuilder
from server.pythonUtils import PythonUtils
from server.core.fileMgr import *
from server.features.autocomplete import AutoCompleteFeature
from server.features.definition import DefinitionFeature
from server.features.hover import HoverFeature
import urllib.parse
import urllib.request

from lsprotocol.types import *
from lsprotocol.types import (WorkDoneProgressBegin,
                                WorkDoneProgressEnd,
                                WorkDoneProgressReport)
from .constants import *

COUNT_DOWN_START_IN_SECONDS = 10
COUNT_DOWN_SLEEP_IN_SECONDS = 1

@odoo_server.feature(SHUTDOWN)
@send_error_on_traceback
def shutdown(ls):
    if Odoo.get():
        ls.show_message_log("Interrupting initialization", MessageType.Log)
        Odoo.get().interrupt_initialization()
        ls.show_message_log("Reset existing database", MessageType.Log)
        Odoo.get().reset(ls)

@odoo_server.feature(TEXT_DOCUMENT_COMPLETION, CompletionOptions(trigger_characters=[',', '.', '"', "'"]))
def completions(ls, params: Optional[CompletionParams] = None) -> CompletionList:
    """Returns completion items."""
    if not params:
        ls.show_message_log("Impossible autocompletion: no params provided", MessageType.Error)
        return None
    ls.show_message_log("Completion requested on " + params.text_document.uri + " at " + str(params.position.line) + ":" + str(params.position.character), MessageType.Log)
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = FileMgr.uri2pathname(params.text_document.uri)
    if not path.endswith(".py"):
        return None
    with Odoo.get().acquire_read(timeout=1) as acquired:
        if acquired:
            return AutoCompleteFeature.autocomplete(path, content, params.position.line+1, params.position.character+1)

@odoo_server.feature(TEXT_DOCUMENT_HOVER)
def hover(ls, params: TextDocumentPositionParams):
    ls.show_message_log("Hover requested on " + params.text_document.uri + " at " + str(params.position.line) + ":" + str(params.position.character), MessageType.Log)
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = FileMgr.uri2pathname(params.text_document.uri)
    if not path.endswith(".py"):
        return None
    with Odoo.get().acquire_read(timeout=1) as acquired:
        if acquired:
            file_symbol = Odoo.get().get_file_symbol(path)
            if file_symbol and params.text_document.uri[-3:] == ".py":
                #Force the parsoTree to be loaded by giving file content and opened==True
                parsoTree = FileMgr.getFileInfo(path, content, opened=True).parso_tree
                return HoverFeature.get_Hover(file_symbol, parsoTree, params.position.line + 1, params.position.character + 1)
        else:
            content = MarkupContent(
                kind=MarkupKind.Markdown,
                value="Odoo extension is loading, please wait..."
            )
            return Hover(
                contents=content
            )
    return None

@odoo_server.feature(TEXT_DOCUMENT_DEFINITION)
def definition(ls, params: TextDocumentPositionParams):
    """Returns the location of a symbol definition"""
    ls.show_message_log("Definition requested on " + params.text_document.uri + " at " + str(params.position.line) + ":" + str(params.position.character), MessageType.Log)
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = FileMgr.uri2pathname(params.text_document.uri)
    with Odoo.get().acquire_read(timeout=2) as acquired:
        if acquired:
            file_symbol = Odoo.get().get_file_symbol(path)
            if file_symbol and params.text_document.uri[-3:] == ".py":
                #Force the parsoTree to be loaded by giving file content and opened==True
                parsoTree = FileMgr.getFileInfo(path, content, opened=True).parso_tree
                return DefinitionFeature.get_location(file_symbol, parsoTree, params.position.line + 1, params.position.character + 1)

def _did_change_after_delay(ls, params: DidChangeTextDocumentParams, reg_id):
    try:
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
        odoo_server.launch_thread(target=Odoo.get().file_change, args=(ls, final_path, source, params.text_document.version))
    except Exception:
        odoo_server.show_message_log(traceback.format_exc(), MessageType.Error)
        odoo_server.send_notification("Odoo/displayCrashNotification", {"crashInfo": traceback.format_exc()})

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
    for f in params.files:
        old_path = urllib.parse.urlparse(urllib.parse.unquote(f.old_uri)).path
        old_path = urllib.request.url2pathname(old_path)
        new_path = urllib.parse.urlparse(urllib.parse.unquote(f.new_uri)).path
        new_path = urllib.request.url2pathname(new_path)
        #TODO find better than this small hack for windows (get disk letter in capital)
        if os.name == "nt":
            old_path = old_path[0].capitalize() + old_path[1:]
            new_path = new_path[0].capitalize() + new_path[1:]
        odoo_server.launch_thread(target=Odoo.get().file_rename, args=(ls, old_path, new_path))

@odoo_server.feature(WORKSPACE_DID_DELETE_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
def did_delete_files(ls, params: DeleteFilesParams):
    for f in params.files:
        path = urllib.parse.urlparse(urllib.parse.unquote(f.uri)).path
        path = urllib.request.url2pathname(path)
        #TODO find better than this small hack for windows (get disk letter in capital)
        if os.name == "nt":
            path = path[0].capitalize() + path[1:]
        odoo_server.launch_thread(target=Odoo.get().file_rename, args=(ls, path, ""))

@odoo_server.feature(WORKSPACE_DID_CREATE_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
def did_create_files(ls, params: CreateFilesParams):
    for f in params.files:
        new_path = urllib.parse.urlparse(urllib.parse.unquote(f.uri)).path
        new_path = urllib.request.url2pathname(new_path)
        #TODO find better than this small hack for windows (get disk letter in capital)
        if os.name == "nt":
            new_path = new_path[0].capitalize() + new_path[1:]
        odoo_server.launch_thread(target=Odoo.get().file_rename, args=(ls, "", new_path))

@odoo_server.feature(TEXT_DOCUMENT_DID_CLOSE)
def did_close(server: OdooLanguageServer, params: DidCloseTextDocumentParams):
    """Text document did close notification."""
    path = FileMgr.uri2pathname(params.text_document.uri)
    FileMgr.removeParsoTree(path)

@odoo_server.feature(TEXT_DOCUMENT_DID_OPEN)
def did_open(ls, params: DidOpenTextDocumentParams):
    """Text document did open notification."""
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = FileMgr.uri2pathname(params.text_document.uri)
    f = FileMgr.getFileInfo(path, content, params.text_document.version, opened = True)
    f.publish_diagnostics(ls) #publish for potential syntax errors

@odoo_server.feature("Odoo/configurationChanged")
def client_config_changed(ls: OdooLanguageServer, params=None):
    if Odoo.get():
        ls.show_message_log("Interrupting initialization", MessageType.Log)
        Odoo.get().interrupt_initialization()
        ls.show_message_log("Reset existing database", MessageType.Log)
        Odoo.get().reset(ls)
    FileMgr.files = {}
    ls.show_message_log("Building new database", MessageType.Log)
    odoo_server.launch_thread(target=Odoo.initialize, args=(ls,))

@odoo_server.feature("Odoo/clientReady")
def client_ready(ls, params=None):
    odoo_server.launch_thread(target=Odoo.initialize, args=(ls,))

@odoo_server.feature(WORKSPACE_DID_CHANGE_WORKSPACE_FOLDERS)
def workspace_change_folders(ls, params: DidChangeWorkspaceFoldersParams):
    print("Workspace folders changed")

@odoo_server.feature(WORKSPACE_DIAGNOSTIC)
def workspace_diagnostics(ls, params:WorkspaceDiagnosticParams):
    print("WORKSPACE DIAG")

@odoo_server.feature(TEXT_DOCUMENT_SIGNATURE_HELP)
def document_signature(ls, params: SignatureHelpParams):
    print("Signature help")
