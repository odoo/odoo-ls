use std::{cell::RefCell, collections::{HashMap, HashSet, hash_map, hash_set}, hash::Hash};

use crate::core::symbols::symbol_keys::KeyValidator;

#[derive(Debug, Clone)]
pub struct WeakSet<K: Eq + Hash + Copy> {
    set: HashSet<K>,
    /// Stale keys detected during `iter_valid`. Recorded here instead of being
    /// removed immediately (since `iter_valid` only has `&self`), then flushed
    /// the next time the set is mutated through `insert`. Lazily allocated:
    /// stays `None` until a stale key is actually recorded.
    stale: RefCell<Option<Box<HashSet<K>>>>,
}

impl<K: Eq + Hash + Copy> Default for WeakSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Copy> WeakSet<K> {
    pub fn new() -> Self {
        Self {
            set: HashSet::default(),
            stale: RefCell::new(None),
        }
    }

    pub fn insert(&mut self, key: K) -> bool {
        // Flush stale keys before inserting, so this is the point where the set
        // is kept from accumulating invalid keys over time.
        if let Some(stale) = self.stale.get_mut().take() {
            for k in *stale {
                self.set.remove(&k);
            }
        }
        self.set.insert(key)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.set.contains(key)
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn remove(&mut self, key: &K) {
        self.set.remove(key);
    }

    pub fn clear_invalid(&mut self, table: &impl KeyValidator<K>) {
        self.set.retain(|k| table.is_key_valid(*k));
        // The set no longer holds any invalid key, so pending stale marks are moot.
        *self.stale.get_mut() = None;
    }

    /// Lazily iterates over valid keys. Stale keys are skipped and recorded for
    /// removal on the next `insert`; iteration does not mutate the set itself,
    /// so an early break only pays for the keys actually visited.
    pub fn iter_valid<'a, V: KeyValidator<K>>(&'a self, table: &'a V) -> WeakSetValidIter<'a, K, V> {
        WeakSetValidIter {
            inner: self.set.iter(),
            stale: &self.stale,
            validator: table,
        }
    }

    pub fn retain_valid(&mut self, table: &impl KeyValidator<K>, f: impl Fn(&K) -> bool) {
        self.set.retain(|k| table.is_key_valid(*k) && f(k));
        *self.stale.get_mut() = None;
    }

    pub fn drain_valid<'a>(&'a mut self, table: &'a impl KeyValidator<K>) -> impl Iterator<Item = K> + 'a {
        // The set is about to be emptied, so pending stale marks are moot.
        *self.stale.get_mut() = None;
        self.set.drain().filter(move |k| table.is_key_valid(*k))
    }
}

/// Lazy iterator over the valid keys of a [`WeakSet`], produced by
/// [`WeakSet::iter_valid`]. Invalid keys encountered while iterating are pushed
/// into the set's `stale` accessory for later removal.
pub struct WeakSetValidIter<'a, K: Eq + Hash + Copy, V: KeyValidator<K>> {
    inner: hash_set::Iter<'a, K>,
    stale: &'a RefCell<Option<Box<HashSet<K>>>>,
    validator: &'a V,
}

impl<'a, K: Eq + Hash + Copy, V: KeyValidator<K>> Iterator for WeakSetValidIter<'a, K, V> {
    type Item = K;

    fn next(&mut self) -> Option<K> {
        for &k in self.inner.by_ref() {
            if self.validator.is_key_valid(k) {
                return Some(k);
            }
            self.stale
                .borrow_mut()
                .get_or_insert_with(|| Box::new(HashSet::default()))
                .insert(k);
        }
        None
    }
}

/// Map with weak keys
#[derive(Debug, Clone)]
pub struct WeakMap<K: Eq + Hash + Copy, V> {
    map: HashMap<K, V>,
}

impl<K: Eq + Hash + Copy, V> WeakMap<K, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    pub fn entry(&mut self, key: K) -> hash_map::Entry<'_, K, V> {
        self.map.entry(key)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key)
    }

    /// iterate over values of valid keys
    pub fn iter_valid_values(&mut self, table: &impl KeyValidator<K>) -> impl Iterator<Item = &V> {
        self.map.retain(|k, _| table.is_key_valid(*k));
        self.map.values()
    }
}
