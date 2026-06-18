use lsp_types::{CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol, NumberOrString, Position, Range, SymbolKind};
use serde_json::{Value, json};
use crate::utils::HashMap;
use tracing::{debug, info, warn};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::constants::DiagnosticLevel;
use crate::threads::{NewTsServerDiagnostics, ThreadMessage};

const VIRTUAL_PROJECT_NAME: &str = "odoo-ls-virtual-project";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct TsServerBridge {
    child: Child,
    stdin: ChildStdin,
    /// Pending responses keyed by `request_seq`, filled by the reader thread.
    responses: Arc<Mutex<HashMap<u64, Value>>>,
    /// Notified by the reader thread every time a new response is inserted.
    response_notify: Arc<Condvar>,
    seq: u64,
    root_files: Vec<String>,
    project_paths: HashMap<String, Vec<String>>,
    project_open: bool,
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
    pub fn new(tsserver_path: &str, sender_to_client: crossbeam_channel::Sender<ThreadMessage>) -> std::io::Result<Self> {
        let mut child =
            Command::new("cmd")
                .args(["/c", tsserver_path])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
        info!("tsserver process started (pid {:?})", child.id());

        let responses = Arc::new(Mutex::new(HashMap::default()));
        let response_notify = Arc::new(Condvar::new());
        let stdin = TsServerBridge::start_threads(&mut child, sender_to_client, Arc::clone(&responses), Arc::clone(&response_notify))?;

        Ok(Self {
            child,
            stdin,
            responses,
            response_notify,
            seq: 1,
            root_files: vec![],
            project_paths: HashMap::default(),
            project_open: false,
        })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn new(tsserver_path: &str, sender_to_main: crossbeam_channel::Sender<ThreadMessage>) -> std::io::Result<Self> {
        let mut child = Command::new(tsserver_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        info!("tsserver process started (pid {:?})", child.id());

        let responses = Arc::new(Mutex::new(HashMap::default()));
        let response_notify = Arc::new(Condvar::new());
        let stdin = TsServerBridge::start_threads(&mut child, sender_to_main, Arc::clone(&responses), Arc::clone(&response_notify))?;

        Ok(Self {
            child,
            stdin,
            responses,
            response_notify,
            seq: 1,
            root_files: vec![],
            project_paths: HashMap::default(),
            project_open: false,
        })
    }

    fn start_threads(
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
            loop {
                match read_message_from(&mut reader) {
                    Ok(msg) => handle_message(&sender_to_main, &responses, &response_notify, msg),
                    Err(e) => {
                        warn!("tsserver reader thread error: {}", e);
                        break;
                    }
                }
            }
        });
        Ok(stdin)
    }

    pub fn open_file(&mut self, file_path: &str, file_content: &str) {
        // "open" is a fire-and-forget notification: tsserver never sends a response.
        if !self.root_files.contains(&file_path.to_string()) {
            self.root_files.push(file_path.to_string());
            if self.project_open {
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

    /// Inject a virtual TypeScript declaration file into the project.
    /// Unlike `open_file`, this does not request diagnostics for the file.
    pub fn inject_virtual_declarations(&mut self, file_path: &str, content: &str) {
        if !self.root_files.contains(&file_path.to_string()) {
            self.root_files.push(file_path.to_string());
            if self.project_open {
                self.send_project_command();
            }
        }
        let _ = self.send_request(
            "open",
            json!({
                "file": file_path,
                "fileContent": content,
                "scriptKindName": "TS",
            }),
        );
    }

    /// Send `openExternalProject` so tsserver resolves Odoo module aliases.
    /// `paths` maps import patterns like `"@web/*"` to filesystem globs.
    pub fn open_external_project(&mut self, paths: HashMap<String, Vec<String>>) {
        self.project_paths = paths;
        self.send_project_command();
        self.project_open = true;
    }

    fn send_project_command(&mut self) {
        let root_files: Vec<Value> = self.root_files
            .iter()
            .map(|f| json!({ "fileName": f }))
            .collect();
        let paths_value: serde_json::Map<String, Value> = self.project_paths
            .iter()
            .map(|(k, v)| {
                (k.clone(), json!(v))
            })
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
    ) -> Vec<(String, u32, u32, u32, u32)> {
        let request_seq = match self.send_request(
            "definition",
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
            let file = entry.get("file").and_then(Value::as_str)?.to_string();
            let start = entry.get("start")?;
            let end = entry.get("end")?;
            let start_line = start.get("line").and_then(Value::as_u64)? as u32;
            let start_offset = start.get("offset").and_then(Value::as_u64)? as u32;
            let end_line = end.get("line").and_then(Value::as_u64)? as u32;
            let end_offset = end.get("offset").and_then(Value::as_u64)? as u32;
            // tsserver uses 1-based lines and offsets; convert to 0-based
            Some((file, start_line - 1, start_offset - 1, end_line - 1, end_offset - 1))
        }).collect()
    }

    /// Returns a list of `(file_path, start_line, start_char, end_line, end_char)` tuples
    /// for all references to the symbol at the given position.
    pub fn get_references(
        &mut self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Vec<(String, u32, u32, u32, u32)> {
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
            let file = entry.get("file").and_then(Value::as_str)?.to_string();
            let start = entry.get("start")?;
            let end = entry.get("end")?;
            let start_line = start.get("line").and_then(Value::as_u64)? as u32;
            let start_offset = start.get("offset").and_then(Value::as_u64)? as u32;
            let end_line = end.get("line").and_then(Value::as_u64)? as u32;
            let end_offset = end.get("offset").and_then(Value::as_u64)? as u32;
            // tsserver uses 1-based lines and offsets; convert to 0-based
            Some((file, start_line - 1, start_offset - 1, end_line - 1, end_offset - 1))
        }).collect()
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

    pub fn close_file(&mut self, file_path: &str) {
        let _ = self.send_request("close", json!({ "file": file_path }));
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
            .map(|entry| {
                let label = entry
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let insert_text = entry
                    .get("insertText")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
                let kind = entry
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(ts_kind_to_lsp_kind)
                    .unwrap_or(CompletionItemKind::TEXT);

                CompletionItem {
                    label,
                    insert_text,
                    kind: Some(kind),
                    ..CompletionItem::default()
                }
            })
            .collect()
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
    let event_name = event.get("event").and_then(Value::as_str).unwrap_or("<unknown>");
    debug!("tsserver event '{}'", event_name);
    match event_name {
        "syntaxDiag" | "semanticDiag" | "suggestionDiag" => {
            if let Some(body) = event.get("body") {
                if let Some((file, diagnostics)) = ts_diag_event_to_diagnostics(body) {
                    let diagnostic_level = match_diag_to_diagnostic_level(event_name);
                    let _ = sender_to_main.send(ThreadMessage::NewTsServerDiagnostics(NewTsServerDiagnostics {
                        file,
                        diagnostic_level,
                        diagnostics,
                    }));
                }
            }
        }
        _ => {}
    }
}

fn match_diag_to_diagnostic_level(category: &str) -> DiagnosticLevel {
    match category {
        "syntaxDiag" => DiagnosticLevel::JS_TSSERVER_SYNTAX,
        "semanticDiag" => DiagnosticLevel::JS_TSSERVER_SEMANTIC,
        "suggestionDiag" => DiagnosticLevel::JS_TSSERVER_SUGGESTION,
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

fn nav_node_to_document_symbol(node: &Value) -> Option<DocumentSymbol> {
    let name = node.get("text").and_then(Value::as_str)?;
    if name.is_empty() || name == "<global>" {
        return None;
    }
    let kind_str = node.get("kind").and_then(Value::as_str).unwrap_or("unknown");

    let span = node.get("spans").and_then(Value::as_array)?.first()?;
    let range = nav_span_to_range(span)?;
    let selection_range = node.get("nameSpan")
        .and_then(nav_span_to_range)
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

fn ts_kind_to_symbol_kind(kind: &str) -> SymbolKind {
    match kind {
        "class" | "type" => SymbolKind::CLASS,
        "interface" => SymbolKind::INTERFACE,
        "enum" => SymbolKind::ENUM,
        "enum member" => SymbolKind::ENUM_MEMBER,
        "function" | "local function" => SymbolKind::FUNCTION,
        "method" | "getter" | "setter" => SymbolKind::METHOD,
        "constructor" => SymbolKind::CONSTRUCTOR,
        "property" => SymbolKind::PROPERTY,
        "var" | "const" | "let" | "variable" | "local var" | "parameter" | "alias" => SymbolKind::VARIABLE,
        "module" | "namespace" => SymbolKind::MODULE,
        "script" => SymbolKind::FILE,
        _ => SymbolKind::VARIABLE,
    }
}

fn ts_kind_to_lsp_kind(kind: &str) -> CompletionItemKind {
    match kind {
        "script" => CompletionItemKind::FILE,
        "warning" => CompletionItemKind::TEXT, // warning means that the type in unknown, so use TEXT instead
        "method" => CompletionItemKind::METHOD,
        "function" => CompletionItemKind::FUNCTION,
        "constructor" => CompletionItemKind::CONSTRUCTOR,
        "property" => CompletionItemKind::PROPERTY,
        "class" => CompletionItemKind::CLASS,
        "interface" => CompletionItemKind::INTERFACE,
        "enum" => CompletionItemKind::ENUM,
        "module" => CompletionItemKind::MODULE,
        "keyword" => CompletionItemKind::KEYWORD,
        "variable" | "const" | "let" | "parameter" | "local var" => CompletionItemKind::VARIABLE,
        _ => CompletionItemKind::TEXT,
    }
}
