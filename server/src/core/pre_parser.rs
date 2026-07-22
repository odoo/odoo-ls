//! Background pre-parsing of files used by the module build.
//!
//! During the initial module build (`SyncOdoo::build_modules`) modules are
//! built one at a time in strict dependency order. While the build thread
//! crunches module N (pure CPU on the symbol table), a small pool of worker
//! threads gets ahead of it: for the modules just past N each worker
//!
//! * walks the module dir and pre-parses every `.py`/`.pyi` into an
//!   [`IndexedModule`] (read + ruff parse + noqa scan),
//! * reads each file listed in the manifest's `data` list, and
//! * expands the manifest's `assets` globs, reading the XML files they match and
//!   parsing the JS ones (read + OXC parse + semantic + lint). The expansion itself
//!   is memoized for the build thread — see [`PreParseCache::resolve_assets`].
//!
//! Both results land in a shared [`PreParseCache`] as a [`PreloadedFile`] payload;
//! when the build thread reaches a file it slots the payload in instead of
//! doing the read/parse inline (see `FileInfo::apply_preloaded`).
//!
//! This is strictly best-effort: a cache miss falls back to inline parsing, so
//! correctness never depends on a worker winning the race.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use ruff_source_file::PositionEncoding;

use crate::constants::DEBUG_PRE_PARSER;
use crate::core::file_mgr::{Ast, JS_PARSE_STACK_SIZE, PreloadedFile, hash_text_document, parse_js_inner, parse_python, python_source_type};
use crate::core::symbols::ModuleSymbol;
use crate::core::symbols::symbol_keys::ModuleKey;
use crate::core::text_document::TextDocument;
use crate::threads::SessionInfo;
use crate::utils::{HashMap, HashSet, PathSanitizer};

/// Max number of worker threads parsing files ahead of the build. Workers only have to
/// stay *ahead* of the build thread; past that they merely compete with it for cores,
/// and the build thread is the critical path.
const MAX_PRE_PARSE_WORKERS: usize = 4;

/// Number of worker threads parsing files ahead of the build, sized to the machine.
///
/// We take half of what we are allowed to run on, leaving the rest to the build thread
/// and to the user's own tooling (editor, tsserver, an Odoo server, ...). On an SMT
/// machine this also keeps workers off the build thread's sibling threads, which would
/// slow down the one thread everything else waits on.
///
/// `available_parallelism` is the parallelism available to *this process*: it honours
/// cgroup quotas and CPU affinity, so this stays sane in containers and CI. It is not a
/// count of the machine's cores, and half of it is not a count of its physical ones.
fn n_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .clamp(1, MAX_PRE_PARSE_WORKERS)
}

/// Directory names skipped when walking a module for Python sources.
const SKIPPED_DIRS: &[&str] = &[
    "__pycache__", "static", "node_modules", "migrations", "upgrades",
    "i18n", "views", "data", "security", "description", "doc", "docs",
];

/// One unit of work submitted for the worker pool: a module at position
/// `module_idx` in build order, its on-disk root, the explicit list of
/// data files, and the manifest asset entries it declares
struct Job {
    module_idx: usize,
    module_path: PathBuf,
    data_files: Vec<PathBuf>,
    /// `(owner module path, local url)` per manifest asset entry, as produced by
    /// [`ModuleSymbol::asset_entries`]. Still unexpanded: the glob walk happens in
    /// the worker, off the build thread ([`PreParseCache::resolve_assets`]).
    asset_entries: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct IndexedStore {
    // the actual pre-parsed payloads
    files_by_path: HashMap<String, PreloadedFile>,
    // accessory index for eviction: the paths produced by each module, keyed by
    // its build index
    paths_by_module_idx: HashMap<usize, Vec<String>>,
}
/// Shared concurrent cache populated by worker threads and drained by the build
/// thread.
#[derive(Debug, Default)]
pub struct PreParseCache {
    index: Mutex<IndexedStore>,
    /// Memoized [`ModuleSymbol::assets_path_resolver`] results, keyed by its arguments.
    /// See [`Self::resolve_assets`].
    #[allow(clippy::type_complexity)]
    resolved_assets: Mutex<HashMap<(String, String), Arc<Vec<PathBuf>>>>,
    /// Asset files already taken by a worker. See [`Self::claim`].
    claimed_assets: Mutex<HashSet<PathBuf>>,
    /// Counters for end-of-build instrumentation. Inert unless
    /// [`DEBUG_PRE_PARSER`] is set; see [`PreParseStats`].
    stats: PreParseStats,
}

impl PreParseCache {
    /// Remove and return the prepared payload for `path`, if a worker produced one.
    pub fn take(&self, path: &str) -> Option<PreloadedFile> {
        let pre_loaded = self.index.lock().unwrap().files_by_path.remove(path);
        self.stats.record_lookup(pre_loaded.is_some());
        pre_loaded
    }

    /// Expand one manifest asset entry to the files it matches, memoizing the result.
    ///
    /// Called by the workers (ahead of the build) and by the build thread itself
    /// (`ModuleSymbol::load_assets`). Both go through
    /// [`ModuleSymbol::asset_entries`], so they always ask for the same keys and the
    /// build thread's own resolve is a map lookup. Whichever thread misses first pays
    /// for the walk; a module the workers skipped is resolved by the build thread and
    /// memoized all the same.
    pub fn resolve_assets(&self, module_path: &str, data_local_url: &str) -> Arc<Vec<PathBuf>> {
        let key = (module_path.to_string(), data_local_url.to_string());
        if let Some(resolved) = self.resolved_assets.lock().unwrap().get(&key) {
            return resolved.clone();
        }
        // Walk the disk outside the lock: two threads racing on the same key just
        // resolve it twice, which is harmless.
        let resolved = Arc::new(ModuleSymbol::assets_path_resolver(module_path, data_local_url));
        self.resolved_assets.lock().unwrap().insert(key, resolved.clone());
        resolved
    }

    /// Take ownership of an asset file, returning `false` if another job got it first.
    ///
    /// Modules routinely list assets they do not own — the standalone webclient bundles
    /// (`project`, `point_of_sale`, `portal`, …) each re-declare large parts of `web`'s
    /// core, and the hottest files are claimed by ~10 modules. The build thread loads
    /// such a file only once, for the first module that reaches it, so preparing it
    /// twice buys nothing: the second payload is parsed, never consumed, then evicted.
    /// Measured on community + enterprise, this drops ~24% of the JS parses and ~45%
    /// of the bytes parsed.
    ///
    /// Claiming for a *later* module than the one that ends up consuming the file is
    /// harmless: payloads are keyed by path, not by module.
    fn claim(&self, path: &Path) -> bool {
        self.claimed_assets.lock().unwrap().insert(path.to_path_buf())
    }

    /// Drop cached entries whose owning module is `module_idx`.
    // In theory, entries could be inserted into the cache after its module's
    // entries have been evicted, and they will stay there until the cache is
    // dropped. Not a problem in practice since workers typically read ahead of
    // the builder thread.
    fn evict_entries(&self, module_idx: usize) {
        let mut index = self.index.lock().unwrap();
        let before = index.files_by_path.len(); // for stats
        if let Some(paths) = index.paths_by_module_idx.remove(&module_idx) {
            for path in paths {
                index.files_by_path.remove(&path);
            }
        }
        self.stats.record_evicted(before - index.files_by_path.len());
    }

    fn insert(&self, module_idx: usize, path: String, file: PreloadedFile) {
        let mut index = self.index.lock().unwrap();
        index.files_by_path.insert(path.clone(), file);
        index.paths_by_module_idx.entry(module_idx).or_default().push(path);
        self.stats.record_insert(index.files_by_path.len());
    }
}

impl Drop for PreParseCache {
    fn drop(&mut self) {
        let unconsumed = self.index.get_mut().map(|i| i.files_by_path.len()).unwrap_or(0);
        self.stats.log(unconsumed);
    }
}

/// Context shared by every worker thread.
#[derive(Clone)]
struct WorkerCtx {
    cache: Arc<PreParseCache>,
    encoding: PositionEncoding,
    test_mode: bool,
}

/// Owns the worker pool and the job queue. Dropping it joins the workers.
pub struct PreParser {
    pub cache: Arc<PreParseCache>,
    last_built_module_idx: Arc<AtomicUsize>,
    job_queue: Arc<Mutex<VecDeque<Job>>>,
    workers: Vec<JoinHandle<()>>,
}

impl PreParser {
    pub fn new(session: &SessionInfo, sorted_modules: &[ModuleKey]) -> Self {
        let cache = Arc::new(PreParseCache::default());
        let last_built_module_idx = Arc::new(AtomicUsize::default());
        let ctx = WorkerCtx {
            cache: cache.clone(),
            encoding: session.sync_odoo.encoding,
            test_mode: session.sync_odoo.test_mode,
        };
        let job_queue = Arc::new(Mutex::new(Self::create_job_queue(session, sorted_modules)));
        let n_workers = n_workers();
        let mut workers = Vec::with_capacity(n_workers);
        // Spawn the worker pool.
        for _ in 0..n_workers {
            let job_queue = job_queue.clone();
            let ctx = ctx.clone();
            let last_built_module_idx = last_built_module_idx.clone();
            let terminate = session.sync_odoo.terminate_rebuild.clone();
            // 8 MiB stack so workers can run parse_js_inner without a nested spawn.
            let worker = std::thread::Builder::new()
                .stack_size(JS_PARSE_STACK_SIZE)
                .spawn(move || {
                    loop {
                        if terminate.load(Ordering::Relaxed) { return; }
                        // Hold the lock only long enough to pull one job, then release
                        // it so a sibling worker can pull while this one parses.
                        let Some(job) = job_queue.lock().unwrap().pop_front() else {
                            return; // no more jobs, end thread
                        };
                        if job.module_idx <= last_built_module_idx.load(Ordering::Relaxed) {
                            // The build has already passed this module, skip it.
                            ctx.cache.stats.record_skipped();
                            continue;
                        }
                        pre_parse_module(&ctx, job);
                    }
                })
            .expect("failed to spawn pre-parse worker");
            workers.push(worker);
        }
        PreParser { cache, last_built_module_idx, job_queue, workers }
    }

    /// Keep track of the build thread's progress so that workers can skip
    /// jobs that are already past if they are running late.
    /// Drops entries for the already built module (as they can no longer be hits).
    pub fn on_module_built(&self, module_idx: usize) {
        self.last_built_module_idx.store(module_idx, Ordering::Relaxed);
        self.cache.evict_entries(module_idx);
    }

    fn create_job_queue(session: &SessionInfo, sorted_modules: &[ModuleKey]) -> VecDeque<Job> {
        sorted_modules.iter().enumerate()
            .skip(1) // skip first module as it can't really parse ahead of builder thread
            .map(|(module_idx, &module_key)| {
                let module = &session.st()[module_key];
                let module_path = PathBuf::from(&module.path);
                let data_files: Vec<PathBuf> = module.data().iter()
                    .map(|(url, _)| module_path.join(url))
                    .collect();
                // The owning module of an asset is looked up here, on the build thread:
                // workers never touch the symbol table.
                let asset_entries = ModuleSymbol::asset_entries(session, module_key).into_iter()
                    .map(|(_, owner_path, local_url)| (owner_path, local_url))
                    .collect();
                Job { module_idx, module_path, data_files, asset_entries }
            })
            .collect()
    }
}

impl Drop for PreParser {
    fn drop(&mut self) {
        // Empty the job queue so that workers end their loop.
        self.job_queue.lock().unwrap().clear();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Process one module job: first read every file in its declared `data` list,
/// then walk the module dir pre-parsing Python sources, then expand and read the
/// manifest assets.
fn pre_parse_module(ctx: &WorkerCtx, job: Job) {
    let Job { module_idx, module_path, data_files, asset_entries } = job;

    // Pass 1: declared data files
    for path in &data_files {
        match path.extension().and_then(|e| e.to_str()) {
            Some("xml") => pre_load_xml(ctx, module_idx, path),
            Some("csv") => pre_load_csv(ctx, module_idx, path),
            _ => {}
        }
    }

    // Pass 2: Python sources
    let mut stack = vec![module_path];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || SKIPPED_DIRS.contains(&name.as_ref()) { continue }
                stack.push(entry.path());
            } else if file_type.is_file() {
                let path = entry.path();
                if matches!(path.extension().and_then(|e| e.to_str()), Some("py") | Some("pyi")) {
                    pre_parse_python(ctx, module_idx, &path);
                }
            }
        }
    }

    // Pass 3: expand manifest assets. Expanding them here keeps the glob walk off the build
    // thread, which finds the result memoized when it reaches this module.
    // Bundles overlap heavily, both within a manifest and across modules, so each file is
    // claimed before being prepared — see [`PreParseCache::claim`].
    let mut xml_assets = vec![];
    let mut js_assets = vec![];
    for (owner_path, local_url) in &asset_entries {
        for path in ctx.cache.resolve_assets(owner_path, local_url).iter() {
            let extension = path.extension().and_then(|e| e.to_str());
            if !matches!(extension, Some("xml") | Some("js")) || !ctx.cache.claim(path) {
                continue;
            }
            match extension {
                Some("xml") => xml_assets.push(path.clone()),
                _ => js_assets.push(path.clone()),
            }
        }
    }

    // Pass 4: read xml assets
    for path in &xml_assets {
        pre_load_xml(ctx, module_idx, path);
    }

    // Pass 5: read and parse js assets
    for path in &js_assets {
        pre_parse_js(ctx, module_idx, path);
    }
}

/// Read and parse a single Python file, depositing the result in the cache.
/// Errors are silently ignored — the build thread will parse the file inline.
fn pre_parse_python(ctx: &WorkerCtx, module_idx: usize, path: &Path) {
    let path_str = path.sanitize();
    let Ok(contents) = fs::read_to_string(path) else { return };
    // Matches `FileMgr::update`: build-time files carry version -1.
    let text_document = TextDocument::new(contents, -1);
    let text_hash = hash_text_document(&text_document);
    let parsed = parse_python(&text_document, python_source_type(&path_str), ctx.encoding, ctx.test_mode, false);
    ctx.cache.insert(module_idx, path_str, PreloadedFile::Python {
        text_hash,
        text_document,
        parsed,
    });
}

/// Read and parse a single JS file, depositing the result in the cache.
/// Errors are silently ignored — the build thread will parse the file inline.
fn pre_parse_js(ctx: &WorkerCtx, module_idx: usize, path: &Path) {
    let path_str = path.sanitize();
    let Ok(contents) = fs::read_to_string(path) else { return };
    // Matches `FileMgr::update`: build-time files carry version -1.
    let text_document = TextDocument::new(contents, -1);
    let text_hash = hash_text_document(&text_document);
    let parsed = parse_js_inner(text_document.contents(), &path_str);
    ctx.cache.insert(module_idx, path_str, PreloadedFile::Js {
        text_hash,
        text_document,
        parsed,
    });
}

fn pre_load_csv(ctx: &WorkerCtx, module_idx: usize, path: &Path) {
    pre_load_data_file(ctx, module_idx, path, Ast::CsvAst);
}

fn pre_load_xml(ctx: &WorkerCtx, module_idx: usize, path: &Path) {
    pre_load_data_file(ctx, module_idx, path, Ast::XmlAst);
}

/// Read a data file (CSV or XML) and store its text and hash in the cache.
fn pre_load_data_file(ctx: &WorkerCtx, module_idx: usize, path: &Path, file_type: Ast) {
    let path_str = path.sanitize();
    // read file from disk
    let Ok(contents) = fs::read_to_string(path) else { return };
    let text_document = TextDocument::new(contents, -1);
    // compute file hash
    let text_hash = hash_text_document(&text_document);
    // store result
    ctx.cache.insert(module_idx, path_str, PreloadedFile::DataFile {
        text_document,
        text_hash,
        ast: file_type,
    });
}

// ==== Instrumentation: only active when DEBUG_PRE_PARSER is set to true  ====

/// Instrumentation counters for [`PreParseCache`].
/// Add methods are no-ops when `DEBUG_PRE_PARSER` is false.
#[derive(Debug, Default)]
struct PreParseStats {
    /// Build-thread lookups that hit a worker-prepared entry (AST + data file).
    hits: AtomicUsize,
    /// Build-thread lookups of an eligible path that found nothing (the build
    /// then parsed/read it inline).
    misses: AtomicUsize,
    /// Files parsed/read by worker threads.
    parsed: AtomicUsize,
    /// Modules skipped because builder thread was ahead of workers — the module
    /// is built without a pre-parse).
    skipped: AtomicUsize,
    /// Entries reclaimed mid-build by `evict_before` because the build moved
    /// past their module without consuming them.
    evicted: AtomicUsize,
    /// High-water mark of live entries in the cache — its peak memory
    /// footprint, which `evict_before` exists to keep bounded.
    peak_live: AtomicUsize,
}

impl PreParseStats {
    /// A cache lookup resolved to a hit (`true`) or a miss (`false`).
    fn record_lookup(&self, hit: bool) {
        if !DEBUG_PRE_PARSER { return; }
        if hit {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A worker inserted one prepared file, leaving `live_len` entries in the
    /// cache.
    fn record_insert(&self, live_len: usize) {
        if !DEBUG_PRE_PARSER { return; }
        self.parsed.fetch_add(1, Ordering::Relaxed);
        self.peak_live.fetch_max(live_len, Ordering::Relaxed);
    }

    /// `evict_before` reclaimed `removed` entries.
    fn record_evicted(&self, removed: usize) {
        if !DEBUG_PRE_PARSER { return; }
        if removed > 0 {
            self.evicted.fetch_add(removed, Ordering::Relaxed);
        }
    }

    /// A worker skipped a module because the build thread was already past it.
    fn record_skipped(&self) {
        if !DEBUG_PRE_PARSER { return; }
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }

    /// Emit the end-of-build summary. `unconsumed` is the number of entries
    /// still live at teardown (parsed but never consumed and never evicted —
    /// their module sits at the tail of the build order).
    fn log(&self, unconsumed: usize) {
        if !DEBUG_PRE_PARSER { return; }
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let parsed = self.parsed.load(Ordering::Relaxed);
        let skipped = self.skipped.load(Ordering::Relaxed);
        let evicted = self.evicted.load(Ordering::Relaxed);
        let peak_live = self.peak_live.load(Ordering::Relaxed);
        let lookups = hits + misses;
        if lookups == 0 && parsed == 0 {
            return;
        }
        let rate = if lookups > 0 { hits as f64 * 100.0 / lookups as f64 } else { 0.0 };
        tracing::info!(
            "pre-parse cache: {hits} hits / {misses} misses ({rate:.1}% hit rate); \
             workers prepared {parsed} files, {skipped} modules skipped"
        );
        tracing::info!(
            "pre-parse memory: peak {peak_live} live entries; \
             {evicted} evicted mid-build, {unconsumed} unconsumed at teardown"
        );
    }
}
