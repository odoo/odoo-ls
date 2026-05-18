//! Background AST pre-parsing.
//!
//! During the initial module build (`SyncOdoo::build_modules` phase 1) modules are
//! built one at a time in strict dependency order. While the build thread crunches
//! module N (pure CPU), a small pool of worker threads parses the Python files of
//! the modules just ahead of it: reading each file, running the ruff parser and
//! building the `IndexedModule`. The result lands in a shared [`PreParseCache`];
//! when the build thread reaches the file it slots the prepared AST in instead of
//! parsing inline (see `FileInfo::update`).
//!
//! This is strictly best-effort: a cache miss simply falls back to inline parsing,
//! so correctness never depends on a worker winning the race. The look-ahead window
//! is kept small because each prepared AST sits in RAM until the build consumes it.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use ruff_python_ast::PySourceType;
use ruff_source_file::PositionEncoding;

use crate::core::file_mgr::{scan_noqa, PreparedAst};
use crate::core::text_document::TextDocument;
use crate::features::node_index_ast::IndexedModule;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;

/// Number of worker threads parsing files ahead of the build. Parsing is heavy,
/// so this stays small — the build thread is the consumer and stays the bottleneck.
const PRE_PARSE_WORKERS: usize = 2;

/// Concurrent `sanitized path -> prepared AST` map, filled by worker threads and
/// drained (once) by the build thread.
#[derive(Debug, Default)]
pub struct PreParseCache {
    map: Mutex<HashMap<String, PreparedAst>>,
}

impl PreParseCache {
    /// Remove and return the prepared AST for `path`, if a worker produced one.
    pub fn take(&self, path: &str) -> Option<PreparedAst> {
        self.map.lock().unwrap().remove(path)
    }

    fn insert(&self, path: String, prepared: PreparedAst) {
        self.map.lock().unwrap().insert(path, prepared);
    }
}

/// Immutable context shared by every worker thread.
struct WorkerCtx {
    cache: Arc<PreParseCache>,
    /// Entry-point paths (all but the public one), sanitized. A file is "external"
    /// — and so skips the noqa scan — when it lives under none of them. Mirrors the
    /// `is_part_of_ep` check in `FileMgr::update_file_info`.
    entry_paths: Vec<String>,
    encoding: PositionEncoding,
    test_mode: bool,
}

/// Owns the worker pool and the channel feeding it module directories. Dropping it
/// closes the channel and joins the workers, so no pre-parse thread outlives phase 1.
pub struct PreParser {
    pub cache: Arc<PreParseCache>,
    sender: Option<SyncSender<PathBuf>>,
    workers: Vec<JoinHandle<()>>,
}

impl PreParser {
    /// Spawn the worker pool. `ahead` is the channel capacity: it bounds how many
    /// modules may be queued at once and so, roughly, how many modules' worth of
    /// ASTs are held in RAM ahead of the build.
    pub fn new(session: &SessionInfo, ahead: usize) -> Self {
        let cache = Arc::new(PreParseCache::default());
        let entry_paths: Vec<String> = session.sync_odoo.entry_point_mgr.borrow()
            .iter_all_but_public()
            .map(|ep| PathBuf::from(&ep.borrow().path).sanitize())
            .collect();
        let ctx = Arc::new(WorkerCtx {
            cache: cache.clone(),
            entry_paths,
            encoding: session.sync_odoo.encoding,
            test_mode: session.sync_odoo.test_mode,
        });
        let (sender, receiver) = sync_channel::<PathBuf>(ahead.max(1));
        let receiver: Arc<Mutex<Receiver<PathBuf>>> = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(PRE_PARSE_WORKERS);
        for _ in 0..PRE_PARSE_WORKERS {
            let receiver = receiver.clone();
            let ctx = ctx.clone();
            workers.push(std::thread::spawn(move || {
                loop {
                    // Hold the lock only long enough to pull one module, then release
                    // it so a sibling worker can pull while this one parses.
                    let module_path = receiver.lock().unwrap().recv();
                    match module_path {
                        Ok(path) => pre_parse_module(&ctx, &path),
                        Err(_) => break, // channel closed: PreParser dropped
                    }
                }
            }));
        }
        PreParser { cache, sender: Some(sender), workers }
    }

    /// Queue a module's directory for pre-parsing. Best-effort: if the workers are
    /// already `ahead` modules behind, the channel is full and the module is
    /// silently skipped — the build thread will parse it inline.
    pub fn submit(&self, module_path: PathBuf) {
        if let Some(sender) = &self.sender {
            match sender.try_send(module_path) {
                Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
            }
        }
    }
}

impl Drop for PreParser {
    fn drop(&mut self) {
        // Close the channel so idle workers wake and exit, then wait for any
        // in-flight parse to finish (each is short).
        self.sender = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Walk a module directory and pre-parse every Python file under it. Mirrors the
/// directory filtering of `file_mgr::prefetch_dir`.
fn pre_parse_module(ctx: &WorkerCtx, module_path: &Path) {
    let mut stack = vec![module_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || name == "__pycache__" || name == "static" || name == "node_modules" {
                    continue;
                }
                stack.push(entry.path());
            } else if file_type.is_file() {
                let path = entry.path();
                match path.extension().and_then(|e| e.to_str()) {
                    Some("py") | Some("pyi") => pre_parse_file(ctx, &path),
                    _ => {}
                }
            }
        }
    }
}

/// Read, parse and index a single Python file, depositing the result in the cache.
/// Errors are silently ignored — the build thread will parse the file inline.
fn pre_parse_file(ctx: &WorkerCtx, path: &Path) {
    let key = path.sanitize();
    let Ok(contents) = fs::read_to_string(path) else { return };
    let source_type = if key.ends_with(".pyi") {
        PySourceType::Stub
    } else {
        PySourceType::Python
    };
    // Matches `FileMgr::update`: build-time files carry version -1.
    let text_document = TextDocument::new(contents, -1);
    let mut hasher = DefaultHasher::new();
    text_document.hash(&mut hasher);
    let text_hash = hasher.finish();
    let parsed = ruff_python_parser::parse_unchecked_source(text_document.contents(), source_type);
    // External files skip the noqa scan, mirroring `FileInfo::_build_ast`.
    let is_external = !ctx.entry_paths.iter().any(|ep| key.starts_with(ep.as_str()));
    let (noqas_blocs, noqas_lines, diag_test_comments) = if is_external {
        (HashMap::new(), HashMap::new(), Vec::new())
    } else {
        let scan = scan_noqa(&parsed, text_document.contents(), &text_document, ctx.encoding, ctx.test_mode);
        (scan.blocs, scan.lines, scan.test_comments)
    };
    let indexed_module = IndexedModule::new(parsed);
    ctx.cache.insert(key, PreparedAst {
        text_hash,
        text_document,
        indexed_module,
        noqas_blocs,
        noqas_lines,
        diag_test_comments,
    });
}
