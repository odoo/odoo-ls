use lsp_types::{CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol, Location, NumberOrString, Position, Range, SymbolKind};
use serde_json::{Value, json};
use crate::features::tsserver_completion::{TsCompletionDetails, entry_to_completion_item, response_to_completion_details};
use crate::utils::{HashMap, HashSet};
use tracing::{debug, info, warn};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::constants::DiagnosticSource;
use crate::core::file_mgr::FileMgr;
use crate::threads::{TsServerDiagnostics, ThreadMessage};

const VIRTUAL_PROJECT_NAME: &str = "odoo-ls-virtual-project";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
/// tsserver can take a while to boot on a cold start (Windows especially, where
/// antivirus scanning of node/tsserver.js is common), so the initial handshake
/// gets a much longer budget than a regular request/response round-trip.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Number of trailing stderr lines kept around to enrich a startup failure message.
const STDERR_HISTORY_LEN: usize = 20;

/**
 * TsserverBridge is a bridge between the LSP server and the tsserver process. It manages the tsserver process, sends requests to it, and receives responses from it.
 * It also handles notifications from tsserver and forwards them to the main thread.
 * All requests to tsserver are blocking for the answer, but the notifications are handled asynchronously in a separate thread.
 * it works by opening an "external project" into tsserver with a adapted tsconfig, which contains all root files, all paths registered for the project ("@odoo/owl" for ex), etc...
 * As this list of file is fixed, we have to open a new project each time the user open a new file, and add it to the list of root files.
 * Only open files are injected into tsserver. All other files are only parsed by oxc.
 */
pub struct TsServerBridge {
    child: Child,
    stdin: ChildStdin,
    /// Pending responses keyed by `request_seq`, filled by the reader thread.
    responses: Arc<Mutex<HashMap<u64, Value>>>,
    /// Notified by the reader thread every time a new response is inserted.
    response_notify: Arc<Condvar>,
    seq: u64,
    /// Files the client has open. Dropped on close (see [`Self::close_file`]); a later
    /// references query re-stages one as a transient root if it still needs it.
    /// Disjoint from [`Self::transient_roots`] — a path is pinned here or evictable there,
    /// never both (the project payload is the union of the two).
    root_files: HashSet<String>,
    /// Evictable roots: reference-expansion files (never `open`ed — read from disk) and the
    /// OWL virtual docs. Dropped as a block once they outgrow the retention budget.
    transient_roots: HashSet<String>,
    /// The subset of [`Self::transient_roots`] we sent an `open` for, and must therefore `close`
    /// on eviction (see [`Self::evict_transient_roots`]).
    open_virtual_docs: HashSet<String>,
    /// Whether `transient_roots` changed since the last `openExternalProject`.
    transient_dirty: bool,
    //contains all tsconfig paths registered for the project, such as "@odoo/owl" as key and resolved paths matching it
    project_paths: HashMap<String, Vec<String>>,
    project_type_roots: Vec<String>,
    project_open: bool,
}


/// A tsserver goto/references result: `(file_path, start_line, start_char, end_line,
/// end_char)`, 0-based (LSP convention). The raw form of an LSP [`Location`];
/// [`ts_to_lsp_location`] converts one into a `Location`.
pub type TsLocation = (String, u32, u32, u32, u32);

/// The two tsserver goto commands taking a position and answering with file spans.
#[derive(Debug, Clone, Copy)]
enum GotoCommand {
    Definition,
    TypeDefinition,
}

impl GotoCommand {
    fn as_str(self) -> &'static str {
        match self {
            GotoCommand::Definition => "definition",
            GotoCommand::TypeDefinition => "typeDefinition",
        }
    }
}

impl std::fmt::Debug for TsServerBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TsServerBridge")
            .field("seq", &self.seq)
            .finish_non_exhaustive()
    }
}

impl TsServerBridge {

    #[cfg(target_os = "windows")]
    pub fn cmd_spawn_tsserver(tsserver_path: &str) -> std::io::Result<Child> {
        // tsserver is usually installed as a `.cmd`/`.bat` shim on Windows, which
        // isn't directly executable; route it through cmd.exe like a shell would.
        Command::new("cmd")
            .args(["/c", tsserver_path])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn cmd_spawn_tsserver(tsserver_path: &str) -> std::io::Result<Child> {
        Command::new(tsserver_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }

    /// Drain the child's stderr in the background so its pipe buffer never fills
    /// up and blocks the process. Everything is logged as it arrives, and the
    /// last few lines are kept around so a startup failure can be reported with
    /// the actual error text instead of a guess.
    fn spawn_stderr_drain(child: &mut Child) -> Arc<Mutex<Vec<String>>> {
        let history = Arc::new(Mutex::new(Vec::new()));
        let Some(stderr) = child.stderr.take() else {
            return history;
        };
        let history_thread = Arc::clone(&history);
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim_end().to_string();
                warn!("tsserver stderr: {}", trimmed);
                let mut history = history_thread.lock().unwrap();
                history.push(trimmed);
                if history.len() > STDERR_HISTORY_LEN {
                    history.remove(0);
                }
                line.clear();
            }
        });
        history
    }

    pub fn new(tsserver_path: &str, sender_to_main: crossbeam_channel::Sender<ThreadMessage>) -> std::io::Result<Self> {
        let mut child = TsServerBridge::cmd_spawn_tsserver(tsserver_path)?;
        info!("tsserver process started (pid {:?})", child.id());

        let stderr_history = TsServerBridge::spawn_stderr_drain(&mut child);

        let responses = Arc::new(Mutex::new(HashMap::default()));
        let response_notify = Arc::new(Condvar::new());
        let stdin = TsServerBridge::start_reader_thread(&mut child, sender_to_main, Arc::clone(&responses), Arc::clone(&response_notify))?;

        let mut bridge = Self {
            child,
            stdin,
            responses,
            response_notify,
            seq: 1,
            root_files: HashSet::default(),
            transient_roots: HashSet::default(),
            open_virtual_docs: HashSet::default(),
            transient_dirty: false,
            project_paths: HashMap::default(),
            project_type_roots: vec![],
            project_open: false,
        };

        bridge.wait_for_startup(tsserver_path, &stderr_history)?;
        bridge.configure();

        Ok(bridge)
    }

    /// Confirm tsserver actually came up by performing a real protocol
    /// round-trip (the lightweight `status` command) instead of guessing from
    /// timing or stderr output. This correctly handles a slow-starting server
    /// (we just keep waiting, up to `STARTUP_TIMEOUT`) as well as a command
    /// that fails immediately (e.g. `cmd.exe` reporting "not recognized") or
    /// one that runs but never speaks the tsserver protocol (the request is
    /// never answered and we time out instead of hanging forever).
    fn wait_for_startup(&mut self, tsserver_path: &str, stderr_history: &Mutex<Vec<String>>) -> std::io::Result<()> {
        let request_seq = self.send_request("status", json!({}))?;
        let deadline = Instant::now() + STARTUP_TIMEOUT;

        loop {
            if let Some(status) = self.child.try_wait()? {
                let message = stderr_history.lock().unwrap().join("\n");
                return Err(std::io::Error::other(format!(
                    "process exited immediately ({status}) while trying to run \"{tsserver_path}\"{}",
                    if message.is_empty() { String::new() } else { format!(": {message}") }
                )));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(std::io::Error::other(format!(
                    "tsserver did not respond to the initial handshake within {:?}; check that \"{tsserver_path}\" is a valid tsserver executable",
                    STARTUP_TIMEOUT
                )));
            }

            let guard = self.responses.lock().unwrap();
            if guard.contains_key(&request_seq) {
                break;
            }
            // Wait in short increments rather than for the full remaining
            // duration so we keep polling for an early process exit above.
            let poll_timeout = remaining.min(Duration::from_millis(100));
            drop(self.response_notify.wait_timeout(guard, poll_timeout).unwrap());
        }

        match self.responses.lock().unwrap().remove(&request_seq) {
            Some(response) if response.get("success").and_then(Value::as_bool).unwrap_or(false) => {
                let version = response.get("body").and_then(|b| b.get("version")).and_then(Value::as_str).unwrap_or("unknown");
                info!("tsserver handshake succeeded (version {})", version);
                Ok(())
            }
            Some(response) => Err(std::io::Error::other(format!(
                "tsserver rejected the initial handshake: {response}"
            ))),
            None => Err(std::io::Error::other("tsserver handshake response vanished unexpectedly")),
        }
    }

    /// Set the session-wide user preferences, once, before any `completions` request.
    /// `includeCompletionsForImportStatements` is the *only* switch enabling import-statement
    /// completions (tsserver reads it from the session config, never per-request); without it
    /// a cursor inside an import clause gets an empty list. The insert-text flag gates the
    /// same code path (matches what VS Code's own TS client sends).
    fn configure(&mut self) {
        let Ok(seq) = self.send_request(
            "configure",
            json!({
                "preferences": {
                    "includeCompletionsForImportStatements": true,
                    "includeCompletionsWithInsertText": true,
                }
            }),
        ) else {
            return;
        };
        // Block on the ack: tsserver serves requests FIFO, but leaving the response unclaimed
        // would strand it in `responses` forever.
        let _ = self.read_response_for_request(seq);
    }

    fn start_reader_thread(
        child: &mut Child,
        sender_to_main: crossbeam_channel::Sender<ThreadMessage>,
        responses: Arc<Mutex<HashMap<u64, Value>>>,
        response_notify: Arc<Condvar>,
    ) -> std::io::Result<ChildStdin> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("Unable to get tsserver stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("Unable to get tsserver stdout"))?;

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut last_warn_at: Option<Instant> = None;
            let mut suppressed_error_count: u32 = 0;
            loop {
                match read_message_from(&mut reader) {
                    Ok(msg) => {
                        last_warn_at = None;
                        suppressed_error_count = 0;
                        handle_message(&sender_to_main, &responses, &response_notify, msg)
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::UnexpectedEof {
                            warn!("tsserver reader thread stopped: {}", e);
                            break;
                        }

                        let now = Instant::now();
                        let should_warn = last_warn_at
                            .map(|last| now.duration_since(last) >= Duration::from_secs(5))
                            .unwrap_or(true);

                        if should_warn {
                            if suppressed_error_count == 0 {
                            } else {
                                warn!("tsserver reader thread error: {}", e);
                            }
                            last_warn_at = Some(now);
                            suppressed_error_count = 0;
                        } else {
                            suppressed_error_count += 1;
                        }
                    }
                }
            }
        });
        Ok(stdin)
    }

    pub fn open_file(&mut self, file_path: &str, file_content: &str) {
        // "open" is a fire-and-forget notification: tsserver never sends a response.
        if self.root_files.insert(file_path.to_string()) {
            // A reference expansion may already have staged this path; it is pinned now, so
            // drop it there rather than list it twice in the project payload.
            self.transient_roots.remove(file_path);
            if self.project_open {
                // As tsserver is not able to add a file to a current project, we have to open a new one with
                // the updated root_files list, so the current file is included in it.
                self.send_project_command();
            }
        }
        let _ = self.send_request(
            "open",
            json!({
                "file": file_path,
                "fileContent": file_content,
            }),
        );
        let _ = self.send_request(
            "geterr",
            json!({
                "delay": 0,
                "files": [file_path],
            }),
        );
    }

    /// Send a virtual doc's content and register it as a transient root, **without**
    /// rebuilding the project — call [`Self::commit_transient_roots`] once the batch is
    /// staged. No `geterr` (a virtual doc must never publish diagnostics); script kind JS
    /// (checkJs expando inference is JS-only); re-staging an open path refreshes its content.
    ///
    /// Content is sent *before* the doc becomes a root on purpose: the commit's synchronous
    /// `updateGraph` then already sees it, so the first request against a new path returns
    /// real results (see `server/docs/owl-virtual-docs.md` §7).
    pub fn stage_virtual_doc(&mut self, file_path: &str, content: &str) {
        let _ = self.send_request(
            "open",
            json!({
                "file": file_path,
                "fileContent": content,
                "scriptKindName": "JS",
            }),
        );
        if self.open_virtual_docs.insert(file_path.to_string()) {
            self.transient_roots.insert(file_path.to_string());
            self.transient_dirty = true;
        }
    }

    /// How many transient roots are currently registered — expansion roots plus open virtual docs.
    /// The caller uses this to decide when the accumulated set has outgrown its budget.
    pub fn transient_root_count(&self) -> usize {
        self.transient_roots.len()
    }

    /// Register files as project roots **without opening them** (tsserver reads them from
    /// disk — no diagnostics for files the user never opened), so a subsequent `references`
    /// request finds the callers living in them. Staged only, see
    /// [`Self::commit_transient_roots`].
    pub fn stage_transient_roots(&mut self, paths: &[String]) {
        for path in paths {
            if !self.root_files.contains(path) {
                self.transient_dirty |= self.transient_roots.insert(path.clone());
            }
        }
    }

    /// Drop every transient root, closing the open virtual docs among them (an *open* file
    /// keeps its `ScriptInfo` alive, so dropping it from `rootFiles` alone frees nothing).
    /// Staged only: evict-then-restage still pays a single project rebuild.
    pub fn evict_transient_roots(&mut self) {
        for path in std::mem::take(&mut self.open_virtual_docs) {
            self.close_file(&path);
        }
        self.transient_dirty |= !self.transient_roots.is_empty();
        self.transient_roots.clear();
    }

    /// One `openExternalProject` covering everything staged since the last commit; a no-op
    /// when nothing changed (every re-send runs a synchronous whole-program rebuild — the
    /// reason staging exists). Before the project exists the staged roots stay pending; the
    /// first `open_external_project` sends the union anyway.
    pub fn commit_transient_roots(&mut self) {
        if self.transient_dirty && self.project_open {
            self.send_project_command();
        }
    }

    /// Send `openExternalProject` so tsserver resolves Odoo module aliases.
    /// `paths` maps import patterns like `"@web/*"` to filesystem globs. `type_roots` are
    /// Odoo's `@types` dirs; setting them explicitly overrides tsserver's default of pulling
    /// every `node_modules/@types` package (jQuery, luxon, …) into every file's global scope.
    pub fn open_external_project(&mut self, paths: HashMap<String, Vec<String>>, type_roots: Vec<String>) {
        self.project_paths = paths;
        self.project_type_roots = type_roots;
        self.send_project_command();
        self.project_open = true;
    }

    /// Re-registers the external project with the current root set. The only place staged transient
    /// roots reach tsserver, hence the only place the dirty flag clears.
    fn send_project_command(&mut self) {
        self.transient_dirty = false;
        // Sorted so the payload — and hence tsserver's program construction — is reproducible.
        let mut names: Vec<&String> = self.root_files.iter().chain(self.transient_roots.iter()).collect();
        names.sort();
        let root_files: Vec<Value> = names
            .into_iter()
            .map(|f| json!({ "fileName": f }))
            .collect();
        let paths_value: serde_json::Map<String, Value> = self.project_paths
            .iter()
            .map(|(k, v)| {
                (k.clone(), json!(v))
            })
            .collect();
        let type_roots_value: Vec<Value> = self.project_type_roots
            .iter()
            .map(|r| json!(r))
            .collect();
        let Ok(seq) = self.send_request(
            "openExternalProject",
            json!({
                "projectFileName": VIRTUAL_PROJECT_NAME,
                "rootFiles": root_files,
                "options": {
                    "allowJs": true,
                    "checkJs": true,
                    "noEmit": true,
                    "moduleResolution": "node",
                    "baseUrl": "",
                    "paths": paths_value,
                    // Explicit typeRoots (Odoo's own `@types` dirs, mirroring
                    // `web/tooling/_jsconfig.json`) override the `node_modules/@types`
                    // default, stopping the ambient luxon/jquery leak.
                    "typeRoots": type_roots_value,
                },
                // No Automatic Type Acquisition: tsserver would otherwise pull `@types/*`
                // from its global cache into every file — huge type graphs checkJs fully
                // instantiates (the dominant RAM cost), and not part of Odoo's type env.
                "typeAcquisition": {
                    "enable": false,
                }
            }),
        ) else {
            return;
        };
        // Wait for tsserver to acknowledge the project update before returning.
        // Without this, subsequent real requests (definition, hover, etc.) race
        // against the project rescan and their responses arrive after the timeout,
        // making every reply appear "one request behind".
        let _ = self.read_response_for_request(seq);
    }

    /// Returns a list of `(file_path, start_line, start_char, end_line, end_char)` tuples
    /// for the definition of the symbol at the given position.
    pub fn get_definition(
        &mut self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Vec<TsLocation> {
        self.goto_targets(GotoCommand::Definition, file_path, line, character)
    }

    /// Same, but for the declaration of the *type* of the symbol at the given position.
    pub fn get_type_definition(
        &mut self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Vec<TsLocation> {
        self.goto_targets(GotoCommand::TypeDefinition, file_path, line, character)
    }

    /// `definition` and `typeDefinition` share a request and a response shape.
    fn goto_targets(
        &mut self,
        command: GotoCommand,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Vec<TsLocation> {
        let request_seq = match self.send_request(
            command.as_str(),
            json!({
                "file": file_path,
                "line": line + 1,
                "offset": character + 1,
            }),
        ) {
            Ok(seq) => seq,
            Err(_) => return vec![],
        };

        let Some(response) = self.read_response_for_request(request_seq) else {
            return vec![];
        };

        if !response.get("success").and_then(Value::as_bool).unwrap_or(false) {
            return vec![];
        }

        let Some(body) = response.get("body").and_then(Value::as_array) else {
            return vec![];
        };

        body.iter().filter_map(|entry| {
            TsServerBridge::value_to_location_tuple(entry)
        }).collect()
    }

    /// Returns a list of `(file_path, start_line, start_char, end_line, end_char)` tuples
    /// for all references to the symbol at the given position.
    pub fn get_references(
        &mut self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Vec<TsLocation> {
        let request_seq = match self.send_request(
            "references",
            json!({
                "file": file_path,
                "line": line + 1,
                "offset": character + 1,
            }),
        ) {
            Ok(seq) => seq,
            Err(_) => return vec![],
        };

        let Some(response) = self.read_response_for_request(request_seq) else {
            return vec![];
        };

        if !response.get("success").and_then(Value::as_bool).unwrap_or(false) {
            return vec![];
        }

        let refs = response
            .get("body")
            .and_then(|b| b.get("refs"))
            .and_then(Value::as_array);

        let Some(refs) = refs else {
            return vec![];
        };

        refs.iter().filter_map(|entry| {
            TsServerBridge::value_to_location_tuple(entry)
        }).collect()
    }

    fn value_to_location_tuple(entry: &Value) -> Option<TsLocation> {
        let file = entry.get("file").and_then(Value::as_str)?.to_string();
        let start = entry.get("start")?;
        let end = entry.get("end")?;
        let start_line = start.get("line").and_then(Value::as_u64)? as u32;
        let start_offset = start.get("offset").and_then(Value::as_u64)? as u32;
        let end_line = end.get("line").and_then(Value::as_u64)? as u32;
        let end_offset = end.get("offset").and_then(Value::as_u64)? as u32;
        // tsserver uses 1-based lines and offsets; convert to 0-based
        Some((file, start_line.saturating_sub(1), start_offset.saturating_sub(1), end_line.saturating_sub(1), end_offset.saturating_sub(1)))
    }

    /// Apply an incremental edit to an already-opened file.
    /// All positions are 0-based (LSP convention); they are converted to
    /// tsserver's 1-based coordinates internally.
    pub fn change_file(
        &mut self,
        file_path: &str,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
        new_text: &str,
    ) {
        // "change" is a fire-and-forget notification like "open".
        let _ = self.send_request(
            "change",
            json!({
                "file": file_path,
                "line":      start_line + 1,
                "offset":    start_char + 1,
                "endLine":   end_line   + 1,
                "endOffset": end_char   + 1,
                "insertString": new_text,
            }),
        );
        let _ = self.send_request(
            "geterr",
            json!({
                "delay": 0,
                "files": [file_path],
            }),
        );
    }

    /// Un-pin a file the client closed. Bounds `root_files`, which would otherwise grow for the
    /// whole session, and costs no references: every query re-derives its roots and re-stages
    /// what is missing (see [`Self::stage_transient_roots`]).
    ///
    /// No `openExternalProject` — a whole-program rebuild on every closed tab is not worth it.
    /// Until the next one, tsserver keeps the file as a root but reads it from disk (that is what
    /// `close` means), so its root set is a stale *superset*: nothing is ever missing from it.
    pub fn close_file(&mut self, file_path: &str) {
        let _ = self.send_request("close", json!({ "file": file_path }));
        self.root_files.remove(file_path);
    }

    /// Returns markdown hover text for the given position, or `None` if tsserver
    /// has no information at that location.
    pub fn get_hover(&mut self, file_path: &str, line: u32, character: u32) -> Option<String> {
        let request_seq = self.send_request(
            "quickinfo",
            json!({
                "file": file_path,
                "line": line + 1,
                "offset": character + 1,
            }),
        ).ok()?;

        let response = self.read_response_for_request(request_seq)?;

        if !response.get("success").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }

        let body = response.get("body")?;

        let display_string = body.get("displayString").and_then(Value::as_str).unwrap_or("");
        let documentation = body.get("documentation").and_then(Value::as_str).unwrap_or("");

        if display_string.is_empty() {
            return None;
        }

        let mut result = format!("```typescript\n{}\n```", display_string);
        if !documentation.is_empty() {
            result.push_str(&format!("\n\n{}", documentation));
        }
        Some(result)
    }

    /// Completions at a position, mapped onto LSP items. `replacementSpan` entries
    /// (import-statement completions) become a `text_edit`; every entry gets
    /// [`TsCompletionResolveData`] so `completionItem/resolve` can fetch its signature and
    /// docs — and, for auto-imports, the import edit.
    pub fn completion_items_for_content(
        &mut self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Vec<CompletionItem> {

        let request_seq = match self.send_request(
            "completions",
            json!({
                "file": file_path,
                "line": line + 1,
                "offset": character + 1,
                "includeExternalModuleExports": true,
                "includeInsertTextCompletions": true,
            }),
        ) {
            Ok(seq) => seq,
            Err(_) => return vec![],
        };

        let Some(response) = self.read_response_for_request(request_seq) else {
            return vec![];
        };

        let success = response
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !success {
            return vec![];
        }

        let Some(body) = response.get("body") else {
            return vec![];
        };

        let entries = body
            .get("entries")
            .and_then(Value::as_array)
            .or_else(|| body.as_array());

        let Some(entries) = entries else {
            return vec![];
        };

        entries
            .iter()
            .map(|entry| entry_to_completion_item(entry, file_path, line, character))
            .collect()
    }

    /// Second half of any completion: the entry's signature and docs, plus — for auto-imports
    /// — the `codeActions` holding the `import … from "…";` edit. Arguments must be the values
    /// of the original `completions` request ([`TsCompletionResolveData`]); edits touching
    /// other files are dropped (LSP's `additionalTextEdits` cannot express them).
    pub fn completion_entry_details(
        &mut self,
        file_path: &str,
        line: u32,
        character: u32,
        name: &str,
        source: Option<&str>,
        data: Option<&Value>,
    ) -> Option<TsCompletionDetails> {
        let mut entry = json!({ "name": name });
        if let Some(source) = source {
            entry["source"] = json!(source);
        }
        if let Some(data) = data {
            entry["data"] = data.clone();
        }

        let request_seq = self.send_request(
            "completionEntryDetails",
            json!({
                "file": file_path,
                "line": line + 1,
                "offset": character + 1,
                "entryNames": [entry],
            }),
        ).ok()?;

        let response = self.read_response_for_request(request_seq)?;
        return response_to_completion_details(response, file_path);
    }

    /// Returns the document symbol tree for a JS/TS file using tsserver's `navtree` command.
    pub fn get_nav_tree(&mut self, file_path: &str) -> Vec<DocumentSymbol> {
        let request_seq = match self.send_request(
            "navtree",
            json!({ "file": file_path }),
        ) {
            Ok(seq) => seq,
            Err(_) => return vec![],
        };

        let Some(response) = self.read_response_for_request(request_seq) else {
            return vec![];
        };

        if !response.get("success").and_then(Value::as_bool).unwrap_or(false) {
            return vec![];
        }

        let Some(body) = response.get("body") else {
            return vec![];
        };

        // The navtree body is a single root NavigationTree node for the file.
        // Top-level symbols are in its `childItems` array.
        if let Some(children) = body.get("childItems").and_then(Value::as_array) {
            children.iter().filter_map(nav_node_to_document_symbol).collect()
        } else if let Some(arr) = body.as_array() {
            // Older tsserver versions may return a flat top-level array.
            arr.iter().filter_map(nav_node_to_document_symbol).collect()
        } else {
            vec![]
        }
    }

    /// Whole-file semantic tokens from tsserver, as `(start, length, token_type, modifiers)`
    /// tuples. `start`/`length` are UTF-16 code-unit offsets into the file; `token_type`
    /// and `modifiers` are **already aligned to our LSP legend** (see `SemanticTokensFeature`),
    /// because our legend's first slots mirror tsserver's numbering — so no translation is
    /// needed here beyond splitting tsserver's packed classification value.
    pub fn get_semantic_tokens(&mut self, file_path: &str) -> Vec<(u32, u32, u32, u32)> {
        // A length past EOF classifies the whole file; tsserver clamps to the real end.
        let request_seq = match self.send_request(
            "encodedSemanticClassifications-full",
            json!({ "file": file_path, "start": 0, "length": i32::MAX, "format": "2020" }),
        ) {
            Ok(seq) => seq,
            Err(_) => return vec![],
        };

        let Some(response) = self.read_response_for_request(request_seq) else {
            return vec![];
        };

        if !response.get("success").and_then(Value::as_bool).unwrap_or(false) {
            return vec![];
        }

        let Some(spans) = response.get("body").and_then(|b| b.get("spans")).and_then(Value::as_array) else {
            return vec![];
        };

        // `spans` is a flat `[start, length, classification, ...]` array. The classification
        // (format "2020") packs the type and modifiers: `type = (class >> 8) - 1`,
        // `modifiers = class & 0xFF`.
        spans
            .chunks_exact(3)
            .filter_map(|triple| {
                let start = triple[0].as_u64()? as u32;
                let length = triple[1].as_u64()? as u32;
                let classification = triple[2].as_u64()? as u32;
                let token_type = (classification >> 8).checked_sub(1)?;
                let modifiers = classification & 0xFF;
                Some((start, length, token_type, modifiers))
            })
            .collect()
    }

    fn send_request(&mut self, command: &str, arguments: Value) -> std::io::Result<u64> {
        let seq = self.seq;
        self.seq += 1;
        let payload = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        debug!("Sent to tsserver: Type: {}, Seq: {}, command: {}", "Request", seq, command);
        self.write_message(&payload)?;
        Ok(seq)
    }

    fn write_message(&mut self, payload: &Value) -> std::io::Result<()> {
        // tsserver reads stdin as newline-delimited JSON (one request per line).
        // Content-Length headers are only used on tsserver's stdout, not stdin.
        let body = serde_json::to_vec(payload)?;
        self.stdin.write_all(&body)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_response_for_request(&mut self, request_seq: u64) -> Option<Value> {
        let deadline = std::time::Instant::now() + RESPONSE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                warn!("tsserver: timeout waiting for response to request seq {}", request_seq);
                return None;
            }
            let guard = self.responses.lock().unwrap();
            // Check before waiting to avoid missing a notification that already fired.
            if guard.contains_key(&request_seq) {
                let mut guard = guard;
                return guard.remove(&request_seq);
            }
            let (mut guard, wait_result) = self.response_notify.wait_timeout(guard, remaining).unwrap();
            if let Some(response) = guard.remove(&request_seq) {
                return Some(response);
            }
            if wait_result.timed_out() {
                warn!("tsserver: timeout waiting for response to request seq {}", request_seq);
                return None;
            }
            // Spurious wake-up — loop and try again.
        }
    }
}

fn handle_message(
    sender_to_main: &crossbeam_channel::Sender<ThreadMessage>,
    responses: &Mutex<HashMap<u64, Value>>,
    response_notify: &Condvar,
    msg: Value,
) {
    match msg.get("type").and_then(Value::as_str) {
        Some("event") => handle_event(sender_to_main, &msg),
        Some("response") => {
            if let Some(seq) = msg.get("request_seq").and_then(Value::as_u64) {
                responses.lock().unwrap().insert(seq, msg);
                response_notify.notify_all();
            } else {
                debug!("tsserver: response missing request_seq: {}", msg);
            }
        }
        other => {
            debug!("tsserver: unexpected message type {:?}: {}", other, msg);
        }
    }
}

fn handle_event(sender_to_main: &crossbeam_channel::Sender<ThreadMessage>, event: &Value) {
    let Some(event_name) = event.get("event").and_then(Value::as_str) else { return };
    match event_name {
        "syntaxDiag" | "semanticDiag" | "suggestionDiag" => {
            if let Some(body) = event.get("body")
            && let Some((file, diagnostics)) = ts_diag_event_to_diagnostics(body) {
                let diagnostic_level = match_diag_to_diagnostic_level(event_name);
                let _ = sender_to_main.send(ThreadMessage::TsServerDiagnostics(TsServerDiagnostics {
                    file,
                    diagnostic_level,
                    diagnostics,
                }));
            }
        }
        _ => {}
    }
}

fn match_diag_to_diagnostic_level(category: &str) -> DiagnosticSource {
    match category {
        "syntaxDiag" => DiagnosticSource::JS_TSSERVER_SYNTAX,
        "semanticDiag" => DiagnosticSource::JS_TSSERVER_SEMANTIC,
        "suggestionDiag" => DiagnosticSource::JS_TSSERVER_SUGGESTION,
        _ => unreachable!(),
    }
}

fn ts_diag_event_to_diagnostics(body: &Value) -> Option<(String, Vec<Diagnostic>)> {
    let file = body.get("file").and_then(Value::as_str)?;
    let diags = body.get("diagnostics").and_then(Value::as_array)?;

    let diagnostics: Vec<Diagnostic> = diags.iter().filter_map(|d| {
        let text = d.get("text").and_then(Value::as_str)?;
        let start = d.get("start")?;
        let end = d.get("end")?;
        let start_line   = start.get("line").and_then(Value::as_u64)? as u32;
        let start_offset = start.get("offset").and_then(Value::as_u64)? as u32;
        let end_line     = end.get("line").and_then(Value::as_u64)? as u32;
        let end_offset   = end.get("offset").and_then(Value::as_u64)? as u32;
        let code = d.get("code")
            .and_then(Value::as_u64)
            .map(|c| NumberOrString::Number(c as i32));
        let severity = match d.get("category").and_then(Value::as_str) {
            Some("error")      => DiagnosticSeverity::ERROR,
            Some("warning")    => DiagnosticSeverity::WARNING,
            Some("suggestion") => DiagnosticSeverity::HINT,
            _                  => DiagnosticSeverity::INFORMATION,
        };
        Some(Diagnostic {
            range: Range {
                start: Position { line: start_line - 1,   character: start_offset - 1 },
                end:   Position { line: end_line   - 1,   character: end_offset   - 1 },
            },
            severity: Some(severity),
            code,
            source: Some("OdooLS-TsServer".to_string()),
            message: text.to_string(),
            ..Diagnostic::default()
        })
    }).collect();

    Some((file.to_string(), diagnostics))
}

fn read_message_from(stdout: &mut BufReader<ChildStdout>) -> std::io::Result<Value> {
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        stdout.read_line(&mut header_line)?;
        if header_line.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "tsserver closed stdout",
            ));
        }
        let trimmed = header_line.trim_end();
        if trimmed.is_empty() {
            if content_length > 0 {
                break;
            }
            // Skip blank lines that appear before the first header (e.g. injected
            // by cmd.exe or left over as a trailing newline after the previous body).
            continue;
        }
        if let Some((name, value)) = trimmed.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    if content_length == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Missing or invalid Content-Length",
        ));
    }

    let mut body = vec![0_u8; content_length];
    stdout.read_exact(&mut body)?;

    let value: Value = serde_json::from_slice(&body)?;
    Ok(value)
}

impl Drop for TsServerBridge {
    fn drop(&mut self) {
        let _ = self.send_request("exit", json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn nav_span_to_range(span: &Value) -> Option<Range> {
    let start = span.get("start")?;
    let end = span.get("end")?;
    let start_line   = start.get("line").and_then(Value::as_u64)? as u32;
    let start_offset = start.get("offset").and_then(Value::as_u64)? as u32;
    let end_line     = end.get("line").and_then(Value::as_u64)? as u32;
    let end_offset   = end.get("offset").and_then(Value::as_u64)? as u32;
    // tsserver uses 1-based lines and offsets; convert to 0-based
    Some(Range {
        start: Position { line: start_line - 1,   character: start_offset - 1 },
        end:   Position { line: end_line   - 1,   character: end_offset   - 1 },
    })
}

fn range_contains(outer: Range, inner: Range) -> bool {
    (outer.start.line < inner.start.line
        || (outer.start.line == inner.start.line && outer.start.character <= inner.start.character))
    && (inner.end.line < outer.end.line
        || (inner.end.line == outer.end.line && inner.end.character <= outer.end.character))
}

fn nav_spans_to_range(spans: &[Value]) -> Option<Range> {
    // Merge all spans into one bounding range so that a nameSpan that falls in
    // a later span (e.g. prototype augmentations) is still contained within the
    // reported fullRange.
    spans.iter()
        .filter_map(nav_span_to_range)
        .reduce(|merged, range| Range {
            start: merged.start.min(range.start),
            end: merged.end.max(range.end),
        })
}

fn nav_node_to_document_symbol(node: &Value) -> Option<DocumentSymbol> {
    let name = node.get("text").and_then(Value::as_str)?;
    if name.is_empty() || name == "<global>" {
        return None;
    }
    let kind_str = node.get("kind").and_then(Value::as_str).unwrap_or("unknown");

    let spans = node.get("spans").and_then(Value::as_array)?;
    let range = nav_spans_to_range(spans)?;
    // nameSpan must be contained within range per the LSP spec; fall back to
    // range if tsserver returns a nameSpan outside the merged fullRange.
    let selection_range = node.get("nameSpan")
        .and_then(nav_span_to_range)
        .filter(|sr| range_contains(range, *sr))
        .unwrap_or(range);

    let children: Option<Vec<DocumentSymbol>> = node
        .get("childItems")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(nav_node_to_document_symbol).collect());

    Some(DocumentSymbol {
        name: name.to_string(),
        detail: None,
        kind: ts_kind_to_symbol_kind(kind_str),
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range,
        selection_range,
        children,
    })
}

/// Maps a tsserver `ScriptElementKind` string to an LSP `SymbolKind`.
///
/// The accepted strings are the values of tsserver's `ScriptElementKind` enum — note that
/// several plausible-looking names are *not* among them (`"variable"` is spelled `"var"`,
/// namespaces are reported as `"module"`).
fn ts_kind_to_symbol_kind(kind: &str) -> SymbolKind {
    match kind {
        "class" | "local class" | "type" => SymbolKind::CLASS,
        "interface" => SymbolKind::INTERFACE,
        "enum" => SymbolKind::ENUM,
        "enum member" => SymbolKind::ENUM_MEMBER,
        "function" | "local function" => SymbolKind::FUNCTION,
        "method" | "call" | "index" => SymbolKind::METHOD,
        "constructor" | "construct" => SymbolKind::CONSTRUCTOR,
        "property" | "getter" | "setter" | "accessor" => SymbolKind::PROPERTY,
        "var" | "const" | "let" | "local var" | "parameter" | "alias"
            | "using" | "await using" => SymbolKind::VARIABLE,
        "type parameter" => SymbolKind::TYPE_PARAMETER,
        "string" => SymbolKind::STRING,
        "module" | "external module name" => SymbolKind::MODULE,
        "script" => SymbolKind::FILE,
        _ => SymbolKind::VARIABLE,
    }
}


/// Maps a tsserver `ScriptElementKind` string to an LSP `CompletionItemKind`.
///
/// Accepts the values of tsserver's `ScriptElementKind` enum; see the note on
/// [`ts_kind_to_symbol_kind`] about names that look plausible but are never emitted.
pub fn ts_kind_to_lsp_kind(kind: &str) -> CompletionItemKind {
    match kind {
        "script" => CompletionItemKind::FILE,
        "directory" => CompletionItemKind::FOLDER,
        "warning" => CompletionItemKind::TEXT, // warning means that the type in unknown, so use TEXT instead
        "method" | "call" | "construct" | "index" => CompletionItemKind::METHOD,
        "function" | "local function" => CompletionItemKind::FUNCTION,
        "constructor" => CompletionItemKind::CONSTRUCTOR,
        "property" | "getter" | "setter" | "accessor" | "JSX attribute" => CompletionItemKind::PROPERTY,
        "class" | "local class" | "type" => CompletionItemKind::CLASS,
        "interface" => CompletionItemKind::INTERFACE,
        "enum" => CompletionItemKind::ENUM,
        "enum member" => CompletionItemKind::ENUM_MEMBER,
        "module" | "external module name" => CompletionItemKind::MODULE,
        "keyword" | "primitive type" => CompletionItemKind::KEYWORD,
        "type parameter" => CompletionItemKind::TYPE_PARAMETER,
        "string" => CompletionItemKind::CONSTANT,
        "var" | "const" | "let" | "parameter" | "local var" | "alias"
            | "using" | "await using" => CompletionItemKind::VARIABLE,
        // Never fall back to TEXT: clients render it as a word suggestion, so an unmapped
        // kind silently looks like editor noise instead of a real symbol.
        _ => CompletionItemKind::VARIABLE,
    }
}

/// A raw tsserver [`TsLocation`] as an LSP `Location`, positions passed through verbatim
/// (tsserver reports UTF-16 — the same assumption the whole JS result path makes).
pub fn ts_to_lsp_location(loc: &TsLocation) -> Location {
    let (file, sl, sc, el, ec) = loc;
    Location {
        uri: FileMgr::pathname2uri(file),
        range: Range {
            start: Position { line: *sl, character: *sc },
            end: Position { line: *el, character: *ec },
        },
    }
}
