use std::collections::hash_map;

use crate::{constants::BuildSteps, core::symbols::symbol_keys::{KeyValidator, SourceFileKey}, fifo_ptr_weak_hash_set::FifoWeakHashSet, utils::HashMap};

/// Steps requested for symbols, keeping the lowest step requested for each symbol, in the request order
#[derive(Debug, Default)]
struct StepRequests {
    steps: HashMap<SourceFileKey, BuildSteps>,
    /// Keys of `steps`, in the request order
    order: Vec<SourceFileKey>,
}

impl StepRequests {
    fn request(&mut self, symbol: SourceFileKey, step: BuildSteps) {
        match self.steps.entry(symbol) {
            hash_map::Entry::Occupied(mut requested) => {
                if step < *requested.get() {
                    requested.insert(step);
                }
            },
            hash_map::Entry::Vacant(entry) => {
                entry.insert(step);
                self.order.push(symbol);
            },
        }
    }

    fn take(&mut self) -> Vec<(SourceFileKey, BuildSteps)> {
        let mut steps = std::mem::take(&mut self.steps);
        std::mem::take(&mut self.order).into_iter()
            .filter_map(|symbol| steps.remove(&symbol).map(|step| (symbol, step)))
            .collect()
    }
}

/// Invalidations requested by `SymbolTable::invalidate` while something is being built. They are applied by
/// `SymbolTable::apply_pending_invalidations` at the end of the build phase (see `BuildScheduler::build_one`).
///
/// Only `symbol_table_impl` can request and apply them: the rest of the crate can only defer them, or hold the
/// invalidations of sub functions (see `BuildScheduler::defer_invalidations` and
/// `BuildScheduler::hold_sub_functions_invalidations`).
#[derive(Debug, Default)]
pub(crate) struct PendingInvalidations {
    /// Number of builds (or other operations, see `BuildScheduler::defer_invalidations`) in progress, that defer
    /// the invalidations
    deferrals: u32,
    /// Symbols to reset, from the lowest step requested
    resets: StepRequests,
    /// Symbols whose dependents have to be invalidated, from the lowest step requested. They are read once the
    /// symbols are reset, as a symbol built meanwhile can register as a dependent (see `SymbolTable::invalidate`).
    dependents: StepRequests,
    /// Files whose sub functions have to be invalidated, while they are not reset themselves
    sub_functions: FifoWeakHashSet<SourceFileKey>,
    /// If set, the invalidations of sub functions wait in `sub_functions` (see `BuildScheduler::hold_sub_functions_invalidations`)
    hold_sub_functions: bool,
}

impl PendingInvalidations {
    /// Start deferring the invalidations, until the matching call to `end_deferral`
    pub(crate) fn begin_deferral(&mut self) {
        self.deferrals += 1;
    }

    /// End a deferral started by `begin_deferral`
    pub(crate) fn end_deferral(&mut self) {
        self.deferrals -= 1;
    }

    /// Hold (or release) the invalidations of sub functions
    pub(crate) fn set_hold_sub_functions(&mut self, hold: bool) {
        self.hold_sub_functions = hold;
    }

    /// Whether the invalidations have to be requested, rather than applied right away
    pub(super) fn are_deferred(&self) -> bool {
        self.deferrals > 0
    }

    /// Request to reset `symbol` to `step`, or keep the lowest step if it is already requested
    pub(super) fn request_reset(&mut self, symbol: SourceFileKey, step: BuildSteps) {
        self.resets.request(symbol, step);
    }

    /// Take the requested resets, in the request order
    pub(super) fn take_resets(&mut self) -> Vec<(SourceFileKey, BuildSteps)> {
        self.resets.take()
    }

    /// Request to invalidate the dependents of `symbol` on its steps from `step`, or keep the lowest step if it
    /// is already requested
    pub(super) fn request_dependents_invalidation(&mut self, symbol: SourceFileKey, step: BuildSteps) {
        self.dependents.request(symbol, step);
    }

    /// Take the requested invalidations of dependents, in the request order
    pub(super) fn take_dependents_invalidations(&mut self) -> Vec<(SourceFileKey, BuildSteps)> {
        self.dependents.take()
    }

    pub(super) fn are_sub_functions_held(&self) -> bool {
        self.hold_sub_functions
    }

    /// Request to invalidate the sub functions of `file`
    pub(super) fn request_sub_functions_invalidation(&mut self, file: SourceFileKey) {
        self.sub_functions.insert(file);
    }

    /// Pop a file whose sub functions have to be invalidated, unless they are held
    pub(super) fn pop_sub_functions_invalidation(&mut self, table: &impl KeyValidator<SourceFileKey>) -> Option<SourceFileKey> {
        if self.hold_sub_functions {
            return None;
        }
        self.sub_functions.pop_front_valid(table)
    }
}
