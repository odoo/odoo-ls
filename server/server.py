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
from server.updateEventQueue import UpdateEvent, EditEvent, UpdateEventType
from server.OdooLanguageServer import OdooLanguageServer, odoo_server
from server.pythonUtils import send_error_on_traceback
import urllib.parse
import urllib.request

from lsprotocol.types import *
from lsprotocol.types import (WorkDoneProgressBegin,
                                WorkDoneProgressEnd,
                                WorkDoneProgressReport)
from .constants import *

COUNT_DOWN_START_IN_SECONDS = 10
COUNT_DOWN_SLEEP_IN_SECONDS = 1


@odoo_server.feature(INITIALIZED)
@send_error_on_traceback
def init(ls, params):
    odoo_server.register_capability(RegistrationParams(
        registrations = [
            Registration(
                id = str(uuid.uuid4()),
                method = WORKSPACE_DID_CHANGE_WATCHED_FILES,
                register_options = DidChangeWatchedFilesRegistrationOptions(watchers = [
                    FileSystemWatcher(glob_pattern = "**", kind = WatchKind.Create | WatchKind.Change | WatchKind.Delete)
                ])
            ),
            Registration(
                id = str(uuid.uuid4()),
                method = WORKSPACE_DID_CHANGE_CONFIGURATION
            ),
        ]
    ))

@odoo_server.feature(SHUTDOWN)
@send_error_on_traceback
def shutdown(ls):
    if Odoo.get():
        ls.show_message_log("Interrupting initialization", MessageType.Log)
        Odoo.get().interrupt_initialization()
        ls.show_message_log("Reset existing database", MessageType.Log)
        Odoo.get().reset(ls)

@odoo_server.feature(TEXT_DOCUMENT_COMPLETION, CompletionOptions(trigger_characters=[',', '.', '"', "'"]))
@send_error_on_traceback
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
@send_error_on_traceback
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

@send_error_on_traceback
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

@odoo_server.feature(TEXT_DOCUMENT_DID_CHANGE)
@send_error_on_traceback
def did_change(ls, params: DidChangeTextDocumentParams):
    """Text document did change notification."""
    if not Odoo.get():
        return
    if Odoo.get().refreshMode != "afterDelay":
        return
    text_doc = ls.workspace.get_document(params.text_document.uri)
    source = text_doc.source
    path = FileMgr.uri2pathname(params.text_document.uri)
    event = EditEvent(ls, path, source, params.text_document.version)
    odoo_server.file_change_event_queue.push(event)

@odoo_server.feature(WORKSPACE_DID_RENAME_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
@send_error_on_traceback
def did_rename_files(ls, params):
    """Workspace did rename files notification."""
    for f in params.files:
        old_path = FileMgr.uri2pathname(f.old_uri)
        new_path = FileMgr.uri2pathname(f.new_uri)
        delete_event = UpdateEvent(ls, old_path, UpdateEventType.DELETE)
        odoo_server.file_change_event_queue.push(delete_event)
        create_event = UpdateEvent(ls, new_path, UpdateEventType.CREATE)
        odoo_server.file_change_event_queue.push(create_event)

@odoo_server.feature(WORKSPACE_DID_CHANGE_WATCHED_FILES)
@send_error_on_traceback
def did_change_watched_files(ls, params: DidChangeWatchedFilesParams):
    """Workspace did change watched files notification."""
    for f in params.changes:
        if ".git" in f.uri:
            continue
        path = FileMgr.uri2pathname(f.uri)
        if f.type == FileChangeType.Created:
            event = UpdateEvent(ls, path, UpdateEventType.CREATE)
            odoo_server.file_change_event_queue.push(event)
        elif f.type == FileChangeType.Deleted:
            event = UpdateEvent(ls, path, UpdateEventType.DELETE)
            odoo_server.file_change_event_queue.push(event)
        elif f.type == FileChangeType.Changed:
            event = EditEvent(ls, path, None, -100)
            odoo_server.file_change_event_queue.push(event)

@odoo_server.feature(WORKSPACE_DID_DELETE_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
@send_error_on_traceback
def did_delete_files(ls, params: DeleteFilesParams):
    for f in params.files:
        path = FileMgr.uri2pathname(f.uri)
        event = UpdateEvent(ls, path, UpdateEventType.DELETE)
        odoo_server.file_change_event_queue.push(event)

@odoo_server.feature(WORKSPACE_DID_CREATE_FILES, FileOperationRegistrationOptions(filters = [
    FileOperationFilter(pattern = FileOperationPattern(glob = "**"))
]))
@send_error_on_traceback
def did_create_files(ls, params: CreateFilesParams):
    for f in params.files:
        new_path = FileMgr.uri2pathname(f.uri)
        event = UpdateEvent(ls, new_path, UpdateEventType.CREATE)
        odoo_server.file_change_event_queue.push(event)

@odoo_server.feature(TEXT_DOCUMENT_DID_CLOSE)
@send_error_on_traceback
def did_close(server: OdooLanguageServer, params: DidCloseTextDocumentParams):
    """Text document did close notification."""
    path = FileMgr.uri2pathname(params.text_document.uri)
    FileMgr.removeParsoTree(path)

@odoo_server.feature(TEXT_DOCUMENT_DID_OPEN)
@send_error_on_traceback
def did_open(ls, params: DidOpenTextDocumentParams):
    """Text document did open notification."""
    text_doc = ls.workspace.get_document(params.text_document.uri)
    content = text_doc.source
    path = FileMgr.uri2pathname(params.text_document.uri)
    f = FileMgr.getFileInfo(path, content, params.text_document.version, opened = True)
    f.publish_diagnostics(ls) #publish for potential syntax errors

@odoo_server.feature("Odoo/configurationChanged")
@send_error_on_traceback
def client_config_changed(ls: OdooLanguageServer, params=None):
    Odoo.reload_database(odoo_server)

@odoo_server.feature("Odoo/clientReady")
@send_error_on_traceback
def client_ready(ls, params=None):
    odoo_server.launch_thread(target=Odoo.initialize, args=(ls,))

@odoo_server.feature(WORKSPACE_DID_CHANGE_WORKSPACE_FOLDERS)
@send_error_on_traceback
def workspace_change_folders(ls, params: DidChangeWorkspaceFoldersParams):
    print("Workspace folders changed")

@odoo_server.feature(WORKSPACE_DIAGNOSTIC)
@send_error_on_traceback
def workspace_diagnostics(ls, params:WorkspaceDiagnosticParams):
    print("WORKSPACE DIAG")

@odoo_server.feature(TEXT_DOCUMENT_SIGNATURE_HELP)
@send_error_on_traceback
def document_signature(ls, params: SignatureHelpParams):
    print("Signature help")

@odoo_server.feature(WORKSPACE_DID_CHANGE_CONFIGURATION)
@send_error_on_traceback
def did_change_configuration(ls, params: DidChangeConfigurationParams):

    def on_change_config(config):
        Odoo.get().refreshMode = config[0]["autoRefresh"]
        Odoo.get().autoSaveDelay = config[0]["autoRefreshDelay"]
        ls.file_change_event_queue.set_delay(Odoo.instance.autoSaveDelay)

    ls.get_configuration(WorkspaceConfigurationParams(items=[
        ConfigurationItem(
            scope_uri='window',
            section="Odoo")
    ]), callback=on_change_config)