use std::{io::Error, panic, sync::{Arc, Mutex, atomic::AtomicBool}, thread::JoinHandle};

use crossbeam_channel::{Receiver, Select, Sender};
use lsp_server::{Connection, IoThreads, Message, ProtocolError, RequestId, ResponseError};
use lsp_types::{CancelParams, CompletionOptions, DefinitionOptions, DocumentSymbolOptions, FileOperationFilter, FileOperationPattern, FileOperationRegistrationOptions, HoverProviderCapability, InitializeParams, InitializeResult, OneOf, ReferencesOptions, SaveOptions, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, WorkDoneProgressOptions, WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities, WorkspaceSymbolOptions, notification::{Cancel, DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders, DidCloseTextDocument, DidCreateFiles, DidDeleteFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, Notification}, request::{Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, References, Request, ResolveCompletionItem, Shutdown, WorkspaceSymbolRequest, WorkspaceSymbolResolve}};
use ruff_source_file::PositionEncoding;
use serde_json::json;
#[cfg(target_os = "linux")]
use nix;
use tracing::{error, info, warn};

// use crate::{constants::{DEBUG_THREADS, EXTENSION_VERSION}, core::{file_mgr::FileMgr, odoo::SyncOdoo}, threads::{delayed_changes_process_thread, message_processor_thread_main, DelayedProcessingMessage}, S, crash_buffer};


#[derive(Debug)]

pub enum ServerError {
    ProtocolError(ProtocolError),
    Serialization(serde_json::Error),
    ServerError(String),
    ResponseError(ResponseError),
}

impl From<ProtocolError> for ServerError {
    fn from(error: ProtocolError) -> Self {
        ServerError::ProtocolError(error)
    }
}

impl From<serde_json::Error> for ServerError {
    fn from(error: serde_json::Error) -> Self {
        ServerError::Serialization(error)
    }
}
