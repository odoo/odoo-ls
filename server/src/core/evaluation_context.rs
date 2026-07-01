use ruff_python_ast::Arguments;
use ruff_text_size::TextRange;
use thin_vec::ThinVec;

use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey, Wk};


/** A context can contain: (non-exhaustive)
* module: the current module the file belongs to
* parent: in an expression, like self.test, the parent is the base attribute, so 'self' for test
* object: the object the expression is executed on (useful if function is defined in parent object).
*/
#[derive(Debug, Clone, Default)]
pub struct Context {
    // ThinVec is a single pointer (8 B inline, x 24 B for a Vec), with no heap
    // allocation while empty. Most contexts are empty.
    entries: ThinVec<(ContextKey, ContextValue)>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextKey {
    Args,
    BaseAttr,
    BaseAttrInserted,
    BaseCall,
    BaseIsSelf,
    Compute,
    ComputeArgRange,
    ComodelName,
    ComodelNameArgRange,
    ConstructingClass,
    Default,
    Delegate,
    FieldParent,
    IsAttrOfInstance,
    IsInValidation,
    Inverse,
    InverseArgRange,
    InverseName,
    InverseNameArgRange,
    Module,
    Parameters,
    ParentFor,
    ParentInstance,
    Range,
    Related,
    RelatedArgRange,
    Required,
    Search,
    SearchArgRange,
    // placeholder after removal (tombstone) - should not be used as key
    EMPTY,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextValue {
    BOOLEAN(bool),
    STRING(String),
    MODULE(Wk<ModuleKey>),
    SYMBOL(Wk<SymbolKey>),
    ARGUMENTS(Arguments),
    RANGE(TextRange),
    // empty value after removal - should not be used as value
    EMPTY,
}

impl FromIterator<(ContextKey, ContextValue)> for Context {
    /// Builds a Context from an iterable of key-value pairs. Keys must be unique.
    fn from_iter<T: IntoIterator<Item = (ContextKey, ContextValue)>>(iter: T) -> Self {
        let entries: ThinVec<_> = iter.into_iter().collect();
        debug_assert!({
            let unique_keys = entries.iter().map(|(k, _)| *k).collect::<crate::utils::HashSet<_>>();
            unique_keys.len() == entries.len()
            }, "Context::from_iter requires unique keys"
        );
        debug_assert!(
            entries.iter().all(|(k, _)| *k != ContextKey::EMPTY),
            "Context::from_iter must not receive EMPTY keys"
        );
        Context { entries }
    }
}

impl Context {
    pub fn insert(&mut self, key: ContextKey, value: ContextValue) {
        let mut empty_slot = None;
        // update value if key already exists
        for (i, (k, v)) in self.entries.iter_mut().enumerate() {
            if *k == key {
                *v = value;
                return;
            }
            if empty_slot.is_none() && *k == ContextKey::EMPTY {
                empty_slot = Some(i)
            }
        }
        // otherwise, insert new entry
        // reuse empty slot if any
        if let Some(i) = empty_slot {
            self.entries[i] = (key, value);
        } else {
            self.entries.push((key, value));
        }
    }

    /// Removing an entry that was added last is (usually) more efficient.
    pub fn remove(&mut self, key: ContextKey) -> Option<ContextValue> {
        while let Some((k, _)) = self.entries.last() {
            if *k == key {
                // key matches last entry, simply pop it
                return self.entries.pop().map(|(_, v)| v);
            } else if *k == ContextKey::EMPTY {
                // last entry is empty: clean it up and keep seaching
                self.entries.pop();
            } else {
                break;
            }
        }
        // We keep seaching in reverse order, skip last entry (it was checked above)
        for (k, v) in self.entries.iter_mut().rev().skip(1) {
            if *k == key {
                *k = ContextKey::EMPTY; // mark as empty
                let value = std::mem::replace(v, ContextValue::EMPTY);
                return Some(value);
            }
        }
        None
    }

    pub fn get(&self, key: ContextKey) -> Option<&ContextValue> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &ContextKey) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(ContextKey, ContextValue)> {
        self.entries.iter().filter(|(k, _)| *k != ContextKey::EMPTY)
    }

    /// Merges two Contexts into a new one.
    /// When a key is present in both, value from `b` has precedence.
    pub fn merge(a: &Self, b: &Self) -> Self {
        let mut result = ThinVec::with_capacity(a.entries.len() + b.entries.len());
        result.extend(a.iter().filter(|(key, _)| !b.contains_key(key)).cloned());
        result.extend(b.iter().cloned());
        Self { entries: result }
    }
}

impl ContextValue {
    pub fn as_bool(&self) -> bool {
        match self {
            ContextValue::BOOLEAN(b) => *b,
            _ => panic!("Not a boolean")
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ContextValue::STRING(s) => s,
            _ => panic!("Not a string")
        }
    }

    pub fn as_symbol(&self) -> Wk<SymbolKey> {
        match self {
            ContextValue::SYMBOL(s) => *s,
            _ => panic!("Not a symbol")
        }
    }

    pub fn as_text_range(&self) -> TextRange {
        match self {
            ContextValue::RANGE(r) => *r,
            _ => panic!("Not a TextRange")
        }
    }

    pub fn as_arguments(&self) -> Arguments {
        match self {
            ContextValue::ARGUMENTS(a) => a.clone(),
            _ => panic!("Not an arguments")
        }
    }
}

impl PartialEq for Context {
    fn eq(&self, other: &Self) -> bool {
        self.iter().count() == other.iter().count()
            && self.iter().all(|(k, v)| other.get(*k) == Some(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_inserted_value() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        assert_eq!(ctx.get(ContextKey::Required), Some(&ContextValue::BOOLEAN(true)));
    }

    #[test]
    fn get_missing_key_returns_none() {
        let ctx = Context::default();
        assert_eq!(ctx.get(ContextKey::Required), None);
    }

    #[test]
    fn insert_updates_existing_key() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Default, ContextValue::STRING("a".to_string()));
        ctx.insert(ContextKey::Default, ContextValue::STRING("b".to_string()));
        assert_eq!(ctx.get(ContextKey::Default), Some(&ContextValue::STRING("b".to_string())));
        // update must not create a second entry
        assert_eq!(ctx.iter().count(), 1);
    }

    #[test]
    fn contains_key_reflects_presence() {
        let mut ctx = Context::default();
        assert!(!ctx.contains_key(&ContextKey::Search));
        ctx.insert(ContextKey::Search, ContextValue::BOOLEAN(false));
        assert!(ctx.contains_key(&ContextKey::Search));
    }

    #[test]
    fn remove_returns_value_and_clears_key() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Inverse, ContextValue::STRING("x".to_string()));
        assert_eq!(ctx.remove(ContextKey::Inverse), Some(ContextValue::STRING("x".to_string())));
        assert!(!ctx.contains_key(&ContextKey::Inverse));
        assert_eq!(ctx.get(ContextKey::Inverse), None);
    }

    #[test]
    fn remove_missing_key_returns_none() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        assert_eq!(ctx.remove(ContextKey::Search), None);
        // unrelated entry is untouched
        assert!(ctx.contains_key(&ContextKey::Required));
    }

    #[test]
    fn remove_non_last_entry_keeps_others() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        ctx.insert(ContextKey::Search, ContextValue::BOOLEAN(false));
        // remove a middle (non-last) entry
        assert_eq!(ctx.remove(ContextKey::Default), Some(ContextValue::STRING("d".to_string())));
        assert!(!ctx.contains_key(&ContextKey::Default));
        assert!(ctx.contains_key(&ContextKey::Required));
        assert!(ctx.contains_key(&ContextKey::Search));
        assert_eq!(ctx.iter().count(), 2);
    }

    #[test]
    fn iter_skips_tombstones() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        // non-last removal leaves an EMPTY tombstone in raw storage
        ctx.remove(ContextKey::Required);
        assert_eq!(ctx.entries.len(), 2);
        // iter must hide the tombstone and only yield the live entry
        let yielded: Vec<_> = ctx.iter().collect();
        assert_eq!(yielded, vec![&(ContextKey::Default, ContextValue::STRING("d".to_string()))]);
    }

    #[test]
    fn insert_reuses_emptied_slot() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        // removing the non-last entry leaves an EMPTY slot to be reused
        ctx.remove(ContextKey::Required);
        ctx.insert(ContextKey::Search, ContextValue::BOOLEAN(false));
        assert_eq!(ctx.entries.len(), 2);
        assert_eq!(ctx.iter().count(), 2);
        assert!(ctx.contains_key(&ContextKey::Search));
    }

    #[test]
    fn remove_then_reinsert_same_key() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        // non-last removal tombstones Required
        ctx.remove(ContextKey::Required);
        // putting it back reuses the freed slot rather than growing storage
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(false));
        assert_eq!(ctx.entries.len(), 2);
        assert_eq!(ctx.iter().count(), 2);
        assert_eq!(ctx.get(ContextKey::Required), Some(&ContextValue::BOOLEAN(false)));
    }

    #[test]
    fn insert_reuses_first_of_multiple_tombstones() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        ctx.insert(ContextKey::Search, ContextValue::BOOLEAN(false));
        // tombstone the two non-last entries -> two EMPTY slots in storage
        ctx.remove(ContextKey::Required);
        ctx.remove(ContextKey::Default);
        assert_eq!(ctx.entries.len(), 3);
        // one insert fills one tombstone; storage must not grow
        ctx.insert(ContextKey::Inverse, ContextValue::BOOLEAN(true));
        assert_eq!(ctx.entries.len(), 3);
        // it reused the first tombstone (Required's slot, index 0)
        assert_eq!(ctx.entries[0].0, ContextKey::Inverse);
        assert_eq!(ctx.iter().count(), 2);
    }

    #[test]
    fn double_remove_returns_none_second_time() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Inverse, ContextValue::STRING("x".to_string()));
        assert!(ctx.remove(ContextKey::Inverse).is_some());
        assert_eq!(ctx.remove(ContextKey::Inverse), None);
    }

    #[test]
    fn pop_last_leaves_no_tombstone() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        // removing the only (last) entry pops it outright, no tombstone left behind
        ctx.remove(ContextKey::Required);
        assert_eq!(ctx.entries.len(), 0);
    }

    #[test]
    fn lifo_removal_clears_all_entries() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        ctx.insert(ContextKey::Search, ContextValue::BOOLEAN(false));
        // pop in reverse insertion order: each removal hits the last entry
        ctx.remove(ContextKey::Search);
        assert_eq!(ctx.entries.len(), 2);
        ctx.remove(ContextKey::Default);
        assert_eq!(ctx.entries.len(), 1);
        ctx.remove(ContextKey::Required);
        // fully drained, raw storage reclaimed (no tombstones)
        assert_eq!(ctx.entries.len(), 0);
    }

    #[test]
    fn next_remove_sweeps_trailing_tombstone() {
        let mut ctx = Context::default();
        ctx.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        ctx.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        ctx.insert(ContextKey::Search, ContextValue::BOOLEAN(false));
        // remove a middle entry: leaves a tombstone, raw storage unchanged
        ctx.remove(ContextKey::Default);
        assert_eq!(ctx.entries.len(), 3);
        // remove the last entry: it pops & returns immediately, so the middle
        // tombstone is now trailing but still occupies storage
        ctx.remove(ContextKey::Search);
        assert_eq!(ctx.entries.len(), 2);
        // the next removal sweeps that trailing tombstone before popping its target
        ctx.remove(ContextKey::Required);
        assert_eq!(ctx.entries.len(), 0);
    }

    #[test]
    fn from_iter_builds_context_with_entries() {
        let ctx = Context::from_iter([
            (ContextKey::Required, ContextValue::BOOLEAN(true)),
            (ContextKey::Default, ContextValue::STRING("d".to_string())),
        ]);
        assert_eq!(ctx.iter().count(), 2);
        assert_eq!(ctx.get(ContextKey::Required), Some(&ContextValue::BOOLEAN(true)));
        assert_eq!(ctx.get(ContextKey::Default), Some(&ContextValue::STRING("d".to_string())));
    }

    #[test]
    fn from_iter_empty_yields_empty_context() {
        let ctx = Context::from_iter([]);
        assert_eq!(ctx.iter().count(), 0);
        assert_eq!(ctx, Context::default());
    }

    #[test]
    fn merge_unions_disjoint_keys() {
        let a = Context::from_iter([(ContextKey::Required, ContextValue::BOOLEAN(true))]);
        let b = Context::from_iter([(ContextKey::Search, ContextValue::BOOLEAN(false))]);
        let merged = Context::merge(&a, &b);
        assert_eq!(merged.iter().count(), 2);
        assert_eq!(merged.get(ContextKey::Required), Some(&ContextValue::BOOLEAN(true)));
        assert_eq!(merged.get(ContextKey::Search), Some(&ContextValue::BOOLEAN(false)));
    }

    #[test]
    fn merge_b_takes_precedence_on_conflict() {
        let a = Context::from_iter([(ContextKey::Default, ContextValue::STRING("a".to_string()))]);
        let b = Context::from_iter([(ContextKey::Default, ContextValue::STRING("b".to_string()))]);
        let merged = Context::merge(&a, &b);
        // conflicting key keeps b's value, with no duplicate entry
        assert_eq!(merged.iter().count(), 1);
        assert_eq!(merged.get(ContextKey::Default), Some(&ContextValue::STRING("b".to_string())));
    }

    #[test]
    fn merge_with_empty_is_identity() {
        let ctx = Context::from_iter([
            (ContextKey::Required, ContextValue::BOOLEAN(true)),
            (ContextKey::Default, ContextValue::STRING("d".to_string())),
        ]);
        let empty = Context::default();
        // empty is the identity element on both sides
        assert_eq!(Context::merge(&ctx, &empty), ctx);
        assert_eq!(Context::merge(&empty, &ctx), ctx);
        // merging two empties stays empty
        assert_eq!(Context::merge(&empty, &empty), empty);
    }

    #[test]
    fn merge_skips_emptied_slots() {
        let mut a = Context::default();
        a.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        a.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        a.remove(ContextKey::Required); // leaves an EMPTY slot in a
        let b = Context::from_iter([(ContextKey::Search, ContextValue::BOOLEAN(false))]);
        let merged = Context::merge(&a, &b);
        // EMPTY slot must not leak into the merged result
        assert_eq!(merged.iter().count(), 2);
        assert!(!merged.contains_key(&ContextKey::Required));
        assert!(merged.contains_key(&ContextKey::Default));
        assert!(merged.contains_key(&ContextKey::Search));
    }

    #[test]
    fn eq_ignores_entry_order() {
        let a = Context::from_iter([
            (ContextKey::Required, ContextValue::BOOLEAN(true)),
            (ContextKey::Search, ContextValue::BOOLEAN(false)),
        ]);
        let b = Context::from_iter([
            (ContextKey::Search, ContextValue::BOOLEAN(false)),
            (ContextKey::Required, ContextValue::BOOLEAN(true)),
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn eq_distinguishes_values_and_counts() {
        let base = Context::from_iter([(ContextKey::Default, ContextValue::STRING("a".to_string()))]);
        let diff_value = Context::from_iter([(ContextKey::Default, ContextValue::STRING("b".to_string()))]);
        let extra_key = Context::from_iter([
            (ContextKey::Default, ContextValue::STRING("a".to_string())),
            (ContextKey::Required, ContextValue::BOOLEAN(true)),
        ]);
        assert_ne!(base, diff_value);
        assert_ne!(base, extra_key);
    }

    #[test]
    fn eq_ignores_emptied_slots() {
        // A context that had an entry removed should equal a fresh-built equivalent.
        let mut with_empty = Context::default();
        with_empty.insert(ContextKey::Required, ContextValue::BOOLEAN(true));
        with_empty.insert(ContextKey::Default, ContextValue::STRING("d".to_string()));
        with_empty.remove(ContextKey::Required);
        let clean = Context::from_iter([(ContextKey::Default, ContextValue::STRING("d".to_string()))]);
        assert_eq!(with_empty, clean);
    }
}
