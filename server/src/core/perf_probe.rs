//! Ad-hoc, debug-only timing probes for investigating where time goes when a
//! `Function` symbol (typically a class method) is built individually,
//! outside of its file's single whole-file pass - see `EAGER_METHOD_ARCH_BUILD`
//! in `constants.rs`. Zero-cost (branches on a `bool` const) when
//! [`crate::constants::DEBUG_PERF_PROBE`] is `false`.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tracing::info;

use crate::constants::DEBUG_PERF_PROBE;

/// One individually-built (`file_mode == false`) function's real AST-walk
/// evaluation, with enough context to later ask "did anything actually need
/// this, or was it wasted work?" - see `record_ast_walk`.
#[derive(Debug, Serialize)]
pub struct AstWalkRecord {
    pub step: String,
    pub symbol: String,
    pub function_is_external: bool,
    pub file_is_external: bool,
    pub file_in_workspace: bool,
    pub file_opened: bool,
}

pub static AST_WALK_RECORDS: Mutex<Vec<AstWalkRecord>> = Mutex::new(Vec::new());

pub fn record_ast_walk(rec: AstWalkRecord) {
    if !DEBUG_PERF_PROBE { return; }
    AST_WALK_RECORDS.lock().unwrap().push(rec);
}

pub fn dump_ast_walk_records(path: &str) {
    if !DEBUG_PERF_PROBE { return; }
    let records = AST_WALK_RECORDS.lock().unwrap();
    match serde_json::to_string_pretty(&*records) {
        Ok(json) => match std::fs::write(path, json) {
            Ok(()) => info!("PERF_PROBE dumped {} ast-walk records to {path}", records.len()),
            Err(e) => tracing::error!("PERF_PROBE failed to write {path}: {e}"),
        },
        Err(e) => tracing::error!("PERF_PROBE failed to serialize ast-walk records: {e}"),
    }
}

/// Total wall time spent inside `PythonArchEval::eval_arch` for individually
/// built functions (`file_mode == false`), and how many times it ran.
pub static INDIVIDUAL_EVAL_ARCH_NS: AtomicU64 = AtomicU64::new(0);
pub static INDIVIDUAL_EVAL_ARCH_CALLS: AtomicU64 = AtomicU64::new(0);

/// Of that time, how much was spent re-fetching the (already parsed/cached)
/// file info via `FileMgr::get_or_recreate_file_info`.
pub static FILE_INFO_FETCH_NS: AtomicU64 = AtomicU64::new(0);
pub static FILE_INFO_FETCH_CALLS: AtomicU64 = AtomicU64::new(0);

/// Of that time, how much was spent in the actual AST walk
/// (`visit_sub_stmts` over the function's own body) - the "real work".
pub static AST_WALK_NS: AtomicU64 = AtomicU64::new(0);
pub static AST_WALK_CALLS: AtomicU64 = AtomicU64::new(0);

/// Total wall time spent inside `PythonOdooFunctionAE::build_function_ae`
/// (the ODOO_FUNCTION_AE step), including its own cheap early-return calls
/// for non-methods.
pub static FUNCTION_AE_NS: AtomicU64 = AtomicU64::new(0);
pub static FUNCTION_AE_CALLS: AtomicU64 = AtomicU64::new(0);

/// Total wall time spent in `PythonArchBuilder::visit_func_def`'s eager
/// body-visit + `BuildScheduler::queue` for a *method* specifically (the
/// block `EAGER_METHOD_ARCH_BUILD` gates), i.e. what the prototype fix skips.
pub static METHOD_ARCH_VISIT_NS: AtomicU64 = AtomicU64::new(0);
pub static METHOD_ARCH_VISIT_CALLS: AtomicU64 = AtomicU64::new(0);

/// How many times `PythonValidator::validate_body` actually had to lazily
/// `build_now` a method's ARCH_EVAL / ODOO_FUNCTION_AE step (i.e. it wasn't
/// already done) - a direct measure of how many methods validation itself
/// ever actually needs built, regardless of whether EAGER_METHOD_ARCH_BUILD
/// pre-built them or not.
pub static LAZY_BUILD_NOW_ARCH_EVAL_CALLS: AtomicU64 = AtomicU64::new(0);
pub static LAZY_BUILD_NOW_FUNCTION_AE_CALLS: AtomicU64 = AtomicU64::new(0);

/// RAII scope timer: records elapsed time (and a call count) into the given
/// counters on drop, so it fires on every return path of the scope it's
/// declared in. No-op (no `Instant::now()` call at all) when
/// [`DEBUG_PERF_PROBE`] is `false`.
pub struct ScopeTimer<'a> {
    counter: &'a AtomicU64,
    calls: &'a AtomicU64,
    start: Option<Instant>,
}

impl<'a> ScopeTimer<'a> {
    #[inline]
    pub fn new(counter: &'a AtomicU64, calls: &'a AtomicU64) -> Self {
        Self { counter, calls, start: if DEBUG_PERF_PROBE { Some(Instant::now()) } else { None } }
    }
}

impl Drop for ScopeTimer<'_> {
    #[inline]
    fn drop(&mut self) {
        if let Some(start) = self.start {
            self.counter.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            self.calls.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn log_summary() {
    if !DEBUG_PERF_PROBE { return; }
    let indiv_ns = INDIVIDUAL_EVAL_ARCH_NS.load(Ordering::Relaxed);
    let indiv_calls = INDIVIDUAL_EVAL_ARCH_CALLS.load(Ordering::Relaxed);
    let file_info_ns = FILE_INFO_FETCH_NS.load(Ordering::Relaxed);
    let file_info_calls = FILE_INFO_FETCH_CALLS.load(Ordering::Relaxed);
    let ast_walk_ns = AST_WALK_NS.load(Ordering::Relaxed);
    let ast_walk_calls = AST_WALK_CALLS.load(Ordering::Relaxed);
    let fae_ns = FUNCTION_AE_NS.load(Ordering::Relaxed);
    let fae_calls = FUNCTION_AE_CALLS.load(Ordering::Relaxed);
    info!(
        "PERF_PROBE individual eval_arch: {indiv_calls} calls, {:.3}s total ({:.1}us/call)",
        indiv_ns as f64 / 1e9,
        if indiv_calls > 0 { indiv_ns as f64 / indiv_calls as f64 / 1e3 } else { 0.0 }
    );
    info!(
        "PERF_PROBE   of which file_info fetch: {file_info_calls} calls, {:.3}s total ({:.1}us/call)",
        file_info_ns as f64 / 1e9,
        if file_info_calls > 0 { file_info_ns as f64 / file_info_calls as f64 / 1e3 } else { 0.0 }
    );
    info!(
        "PERF_PROBE   of which ast walk: {ast_walk_calls} calls, {:.3}s total ({:.1}us/call)",
        ast_walk_ns as f64 / 1e9,
        if ast_walk_calls > 0 { ast_walk_ns as f64 / ast_walk_calls as f64 / 1e3 } else { 0.0 }
    );
    info!(
        "PERF_PROBE build_function_ae: {fae_calls} calls, {:.3}s total ({:.1}us/call)",
        fae_ns as f64 / 1e9,
        if fae_calls > 0 { fae_ns as f64 / fae_calls as f64 / 1e3 } else { 0.0 }
    );
    let method_ns = METHOD_ARCH_VISIT_NS.load(Ordering::Relaxed);
    let method_calls = METHOD_ARCH_VISIT_CALLS.load(Ordering::Relaxed);
    info!(
        "PERF_PROBE method ARCH visit+queue: {method_calls} calls, {:.3}s total ({:.1}us/call)",
        method_ns as f64 / 1e9,
        if method_calls > 0 { method_ns as f64 / method_calls as f64 / 1e3 } else { 0.0 }
    );
    info!(
        "PERF_PROBE lazy build_now from validate_body: {} ARCH_EVAL, {} ODOO_FUNCTION_AE",
        LAZY_BUILD_NOW_ARCH_EVAL_CALLS.load(Ordering::Relaxed),
        LAZY_BUILD_NOW_FUNCTION_AE_CALLS.load(Ordering::Relaxed),
    );
}
