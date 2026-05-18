//! Background AST pre-parsing.
//!
//! During the initial module build (`SyncOdoo::build_modules` phase 1) modules are
//! built one at a time in strict dependency order. While the build thread crunches
//! module N (pure CPU), a small pool of worker threads parses the Python files of
//! the modules just ahead of it: reading each file, running the ruff parser and
//! building the `IndexedModule`. The result lands in a shared [`PreParseCache`];
//! when the build thread reaches the file it slots the prepared AST in instead of
//! parsing inline (see `FileInfo::update`). The same worker pass also fires a
//! `posix_fadvise(WILLNEED)` hint for the module's XML/CSV files — the build reads
//! those inline, so warming the page cache ahead of time is all that is needed.
//!
//! This is strictly best-effort: a cache miss simply falls back to inline parsing,
//! so correctness never depends on a worker winning the race. The look-ahead window
//! is kept small because each prepared AST sits in RAM until the build consumes it.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// Concurrent map of `sanitized path -> (submission index, prepared AST)`, filled
/// by worker threads and drained by the build thread. The submission index is the
/// position of the owning module in the build order; it lets the build thread
/// evict entries for modules it has already passed (see [`Self::evict_before`]),
/// so the map only ever holds the look-ahead window — not every leftover AST.
#[derive(Debug, Default)]
pub struct PreParseCache {
    map: Mutex<HashMap<String, (usize, PreparedAst)>>,
    /// Instrumentation: build-thread lookups that found a worker-prepared AST.
    hits: AtomicUsize,
    /// Instrumentation: build-thread lookups of a `.py`/`.pyi` file that found
    /// nothing (the build thread then parsed it inline).
    misses: AtomicUsize,
    /// Instrumentation: total files parsed by worker threads.
    parsed: AtomicUsize,
    /// Instrumentation: modules dropped by `submit` because the channel was full
    /// (workers were behind — the module is built without a pre-parse).
    rejected: AtomicUsize,
    /// Instrumentation: entries reclaimed mid-build by `evict_before` because the
    /// build had moved past their module without consuming them.
    evicted: AtomicUsize,
    /// Instrumentation: high-water mark of live entries — i.e. the cache's peak
    /// memory footprint, which `evict_before` exists to keep bounded.
    peak_live: AtomicUsize,
}

impl PreParseCache {
    /// Remove and return the prepared AST for `path`, if a worker produced one.
    /// Records the lookup as a hit or a miss for end-of-build instrumentation.
    pub fn take(&self, path: &str) -> Option<PreparedAst> {
        let entry = self.map.lock().unwrap().remove(path);
        if entry.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        entry.map(|(_, prepared)| prepared)
    }

    /// Drop every cached entry whose owning module comes before `module_idx` in
    /// build order. The build proceeds in strict module order, so such entries can
    /// no longer be consumed as hits — keeping them would just grow RAM until the
    /// end of phase 1. Called once per module-build iteration by the build thread.
    pub fn evict_before(&self, module_idx: usize) {
        let mut map = self.map.lock().unwrap();
        let before = map.len();
        map.retain(|_, (idx, _)| *idx >= module_idx);
        let removed = before - map.len();
        drop(map);
        if removed > 0 {
            self.evicted.fetch_add(removed, Ordering::Relaxed);
        }
    }

    fn insert(&self, module_idx: usize, path: String, prepared: PreparedAst) {
        self.parsed.fetch_add(1, Ordering::Relaxed);
        let mut map = self.map.lock().unwrap();
        map.insert(path, (module_idx, prepared));
        self.peak_live.fetch_max(map.len(), Ordering::Relaxed);
    }
}

impl Drop for PreParseCache {
    fn drop(&mut self) {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let parsed = self.parsed.load(Ordering::Relaxed);
        let rejected = self.rejected.load(Ordering::Relaxed);
        let evicted = self.evicted.load(Ordering::Relaxed);
        let peak_live = self.peak_live.load(Ordering::Relaxed);
        let lookups = hits + misses;
        // Entries still live at teardown: parsed but never consumed and never
        // evicted (their module sits at the tail of the build order).
        let unconsumed = self.map.get_mut().map(|m| m.len()).unwrap_or(0);
        if lookups == 0 && parsed == 0 {
            return;
        }
        let rate = if lookups > 0 { hits as f64 * 100.0 / lookups as f64 } else { 0.0 };
        tracing::info!(
            "pre-parse cache: {hits} hits / {misses} misses ({rate:.1}% hit rate); \
             workers parsed {parsed} files, {rejected} modules skipped (channel full)"
        );
        tracing::info!(
            "pre-parse memory: peak {peak_live} live entries; \
             {evicted} evicted mid-build, {unconsumed} unconsumed at teardown"
        );
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
    sender: Option<SyncSender<(usize, PathBuf)>>,
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
        let (sender, receiver) = sync_channel::<(usize, PathBuf)>(ahead.max(1));
        let receiver: Arc<Mutex<Receiver<(usize, PathBuf)>>> = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(PRE_PARSE_WORKERS);
        for _ in 0..PRE_PARSE_WORKERS {
            let receiver = receiver.clone();
            let ctx = ctx.clone();
            workers.push(std::thread::spawn(move || {
                loop {
                    // Hold the lock only long enough to pull one module, then release
                    // it so a sibling worker can pull while this one parses.
                    let job = receiver.lock().unwrap().recv();
                    match job {
                        Ok((module_idx, path)) => pre_parse_module(&ctx, module_idx, &path),
                        Err(_) => break, // channel closed: PreParser dropped
                    }
                }
            }));
        }
        PreParser { cache, sender: Some(sender), workers }
    }

    /// Queue a module's directory for pre-parsing. `module_idx` is the module's
    /// position in build order — workers tag every entry with it so the build can
    /// `evict_before` once it has moved past the module. Best-effort: if the
    /// workers are already `ahead` modules behind, the channel is full and the
    /// module is silently skipped — the build thread will parse it inline.
    pub fn submit(&self, module_idx: usize, module_path: PathBuf) {
        if let Some(sender) = &self.sender {
            match sender.try_send((module_idx, module_path)) {
                Ok(()) | Err(TrySendError::Disconnected(_)) => {}
                Err(TrySendError::Full(_)) => {
                    self.cache.rejected.fetch_add(1, Ordering::Relaxed);
                }
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

/// Walk a module directory once: pre-parse every Python file into the cache, and
/// fire a page-cache prefetch hint for every XML/CSV file (those are read inline
/// by the build, before the Python files of the same module). `migrations/` is
/// skipped entirely — migration scripts are not imported, so the build touches
/// neither their Python nor their data files.
fn pre_parse_module(ctx: &WorkerCtx, module_idx: usize, module_path: &Path) {
    let mut stack = vec![module_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || name == "__pycache__" || name == "static"
                    || name == "node_modules" || name == "migrations" {
                    continue;
                }
                stack.push(entry.path());
            } else if file_type.is_file() {
                let path = entry.path();
                match path.extension().and_then(|e| e.to_str()) {
                    Some("py") | Some("pyi") => pre_parse_file(ctx, module_idx, &path),
                    Some("xml") | Some("csv") => crate::core::file_mgr::fadvise_willneed(&path),
                    _ => {}
                }
            }
        }
    }
}

/// Read, parse and index a single Python file, depositing the result in the cache
/// tagged with `module_idx`. Errors are silently ignored — the build thread will
/// parse the file inline.
fn pre_parse_file(ctx: &WorkerCtx, module_idx: usize, path: &Path) {
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
    ctx.cache.insert(module_idx, key, PreparedAst {
        text_hash,
        text_document,
        indexed_module,
        noqas_blocs,
        noqas_lines,
        diag_test_comments,
    });
}
