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
from json import JSONDecodeError
from typing import Optional
from .odoo import Odoo
from server.pythonParser import PythonParser
from server.pythonUtils import PythonUtils
import urllib.parse
import urllib.request

from lsprotocol.types import (TEXT_DOCUMENT_COMPLETION, TEXT_DOCUMENT_DID_CHANGE,
                               TEXT_DOCUMENT_DID_CLOSE, TEXT_DOCUMENT_DID_OPEN, 
                               TEXT_DOCUMENT_SEMANTIC_TOKENS_FULL, TEXT_DOCUMENT_DEFINITION)
from lsprotocol.types import (CompletionItem, CompletionList, CompletionOptions,
                             CompletionParams, ConfigurationItem,
                             ConfigurationParams, Diagnostic,
                             DidChangeTextDocumentParams,
                             DidCloseTextDocumentParams,
                             DidOpenTextDocumentParams, MessageType, Position,
                             Range, Registration, RegistrationParams,
                             SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
                             Unregistration, UnregistrationParams,
                             TextDocumentPositionParams, Location)
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
        super().__init__(name=EXTENSION_NAME, version=EXTENSION_VERSION)


odoo_server = OdooLanguageServer()


def _validate(ls, params):
    ls.show_message_log('Validating odoo...')

    text_doc = ls.workspace.get_document(params.text_document.uri)

    source = text_doc.source
    diagnostics = _validate_json(source) if source else []

    ls.publish_diagnostics(text_doc.uri, diagnostics)


def _validate_json(source):
    """Validates odoo file."""
    diagnostics = []

    try:
        json.loads(source)
    except JSONDecodeError as err:
        msg = err.msg
        col = err.colno
        line = err.lineno

        d = Diagnostic(
            range=Range(
                start=Position(line=line - 1, character=col - 1),
                end=Position(line=line - 1, character=col)
            ),
            message=msg,
            source=type(odoo_server).__name__
        )

        diagnostics.append(d)

    return diagnostics


@odoo_server.feature(TEXT_DOCUMENT_COMPLETION, CompletionOptions(trigger_characters=[',']))
def completions(params: Optional[CompletionParams] = None) -> CompletionList:
    """Returns completion items."""
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

@odoo_server.feature(TEXT_DOCUMENT_DEFINITION)
def definition(params: TextDocumentPositionParams):
    """Returns the location of a symbol definition"""
    # lookup the ident under the cursor
    #name = get_symbol_name_at_position(params.textDocument.uri, params.position)
    # lookup the ident within the document
    #symbol = lookup_symbol(params.textDocument.uri, name)
    print(params.text_document.uri)
    final_path = urllib.parse.urlparse(urllib.parse.unquote(params.text_document.uri)).path
    final_path = urllib.request.url2pathname(final_path)
    #TODO find better than this small hack for windows
    if os.name == "nt":
        final_path = final_path[0].capitalize() + final_path[1:]
    print(final_path)
    file_symbol = Odoo.get().get_file_symbol(final_path)
    if params.text_document.uri[-3:] == ".py":
        symbol = PythonUtils.getSymbol(file_symbol, params.position.line + 1, params.position.character + 1)
    a = Location(uri="file:///home/odoo/Documents/odoo-servers/false_odoo/odoo/odoo/addons/base/models/ir_model.py", range=Range(start=Position(line=0, character=0), end=Position(line=0, character=0)))
    b = Location(uri="file:///home/odoo/Documents/odoo-servers/false_odoo/odoo/odoo/addons/base/models/ir_actions.py", range=Range(start=Position(line=0, character=0), end=Position(line=0, character=0)))

    return [c, a, b]

@odoo_server.command(CMD_COUNT_DOWN_BLOCKING)
def count_down_10_seconds_blocking(ls, *args):
    """Starts counting down and showing message synchronously.
    It will `block` the main thread, which can be tested by trying to show
    completion items.
    """
    for i in range(COUNT_DOWN_START_IN_SECONDS):
        ls.show_message(f'Counting down... {COUNT_DOWN_START_IN_SECONDS - i}')
        time.sleep(COUNT_DOWN_SLEEP_IN_SECONDS)


@odoo_server.command(CMD_COUNT_DOWN_NON_BLOCKING)
async def count_down_10_seconds_non_blocking(ls, *args):
    """Starts counting down and showing message asynchronously.
    It won't `block` the main thread, which can be tested by trying to show
    completion items.
    """
    for i in range(COUNT_DOWN_START_IN_SECONDS):
        ls.show_message(f'Counting down... {COUNT_DOWN_START_IN_SECONDS - i}')
        await asyncio.sleep(COUNT_DOWN_SLEEP_IN_SECONDS)

@odoo_server.thread()
@odoo_server.feature(TEXT_DOCUMENT_DID_CHANGE)
def did_change(ls, params: DidChangeTextDocumentParams):
    """Text document did change notification."""
    base = Odoo.get(ls)
    _validate(ls, params)


@odoo_server.feature(TEXT_DOCUMENT_DID_CLOSE)
def did_close(server: OdooLanguageServer, params: DidCloseTextDocumentParams):
    """Text document did close notification."""
    server.show_message('Text Document Did Close')

@odoo_server.thread()
@odoo_server.feature(TEXT_DOCUMENT_DID_OPEN)
def did_open(ls, params: DidOpenTextDocumentParams):
    """Text document did open notification."""
    base = Odoo.get(ls)
    base.init_file(params.text_document.uri)

@odoo_server.feature(
    TEXT_DOCUMENT_SEMANTIC_TOKENS_FULL,
    SemanticTokensLegend(
        token_types = ["operator"],
        token_modifiers = []
    )
)
def semantic_tokens(ls: OdooLanguageServer, params: SemanticTokensParams):
    """See https://microsoft.github.io/language-server-protocol/specification#textDocument_semanticTokens
    for details on how semantic tokens are encoded."""
    
    TOKENS = re.compile('".*"(?=:)')
    
    uri = params.text_document.uri
    doc = ls.workspace.get_document(uri)

    last_line = 0
    last_start = 0

    data = []

    for lineno, line in enumerate(doc.lines):
        last_start = 0

        for match in TOKENS.finditer(line):
            start, end = match.span()
            data += [
                (lineno - last_line),
                (start - last_start),
                (end - start),
                0, 
                0
            ]

            last_line = lineno
            last_start = start

    return SemanticTokens(data=data)



@odoo_server.command(CMD_PROGRESS)
async def progress(ls: OdooLanguageServer, *args):
    """Create and start the progress on the client."""
    token = 'token'
    # Create
    await ls.progress.create_async(token)
    # Begin
    ls.progress.begin(token, WorkDoneProgressBegin(title='Indexing', percentage=0))
    # Report
    for i in range(1, 10):
        ls.progress.report(
            token,
            WorkDoneProgressReport(message=f'{i * 10}%', percentage= i * 10),
        )
        await asyncio.sleep(2)
    # End
    ls.progress.end(token, WorkDoneProgressEnd(message='Finished'))


@odoo_server.command(CMD_REGISTER_COMPLETIONS)
async def register_completions(ls: OdooLanguageServer, *args):
    """Register completions method on the client."""
    params = RegistrationParams(registrations=[
                Registration(
                    id=str(uuid.uuid4()),
                    method=TEXT_DOCUMENT_COMPLETION,
                    register_options={"triggerCharacters": "[':']"})
             ])
    response = await ls.register_capability_async(params)
    if response is None:
        ls.show_message('Successfully registered completions method')
    else:
        ls.show_message('Error happened during completions registration.',
                        MessageType.Error)


@odoo_server.command(CMD_SHOW_CONFIGURATION_ASYNC)
async def show_configuration_async(ls: OdooLanguageServer, *args):
    """Gets exampleConfiguration from the client settings using coroutines."""
    try:
        config = await ls.get_configuration_async(
            ConfigurationParams(items=[
                ConfigurationItem(
                    scope_uri='',
                    section=CONFIGURATION_SECTION)
        ]))

        example_config = config[0].get('exampleConfiguration')

        ls.show_message(f'jsonServer.exampleConfiguration value: {example_config}')

    except Exception as e:
        ls.show_message_log(f'Error ocurred: {e}')


@odoo_server.command(CMD_SHOW_CONFIGURATION_CALLBACK)
def show_configuration_callback(ls: OdooLanguageServer, *args):
    """Gets exampleConfiguration from the client settings using callback."""
    def _config_callback(config):
        try:
            example_config = config[0].get('exampleConfiguration')

            ls.show_message(f'jsonServer.exampleConfiguration value: {example_config}')

        except Exception as e:
            ls.show_message_log(f'Error ocurred: {e}')

    ls.get_configuration(ConfigurationParams(items=[
        ConfigurationItem(
            scope_uri='',
            section=CONFIGURATION_SECTION)
    ]), _config_callback)


@odoo_server.thread()
@odoo_server.command(CMD_SHOW_CONFIGURATION_THREAD)
def show_configuration_thread(ls: OdooLanguageServer, *args):
    """Gets exampleConfiguration from the client settings using thread pool."""
    try:
        config = ls.get_configuration(ConfigurationParams(items=[
            ConfigurationItem(
                scope_uri='',
                section=CONFIGURATION_SECTION)
        ])).result(2)

        example_config = config[0].get('exampleConfiguration')

        ls.show_message(f'jsonServer.exampleConfiguration value: {example_config}')

    except Exception as e:
        ls.show_message_log(f'Error ocurred: {e}')


@odoo_server.command(CMD_UNREGISTER_COMPLETIONS)
async def unregister_completions(ls: OdooLanguageServer, *args):
    """Unregister completions method on the client."""
    params = UnregistrationParams(unregisterations=[
        Unregistration(id=str(uuid.uuid4()), method=TEXT_DOCUMENT_COMPLETION)
    ])
    response = await ls.unregister_capability_async(params)
    if response is None:
        ls.show_message('Successfully unregistered completions method')
    else:
        ls.show_message('Error happened during completions unregistration.',
                        MessageType.Error)


