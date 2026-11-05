use std::{cell::RefCell, collections::{hash_map}, hash::Hash, vec::IntoIter};

use crate::{core::symbols::symbol_keys::KeyValidator, utils::{HashMap, HashSet}};

#[derive(Debug, Clone)]
pub struct WeakSet<K: Eq + Hash + Copy> {
    set: RefCell<HashSet<K>>,
}

impl<K: Eq + Hash + Copy> Default for WeakSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Copy> WeakSet<K> {
    pub fn new() -> Self {
        Self {
            set: RefCell::new(HashSet::default()),
        }
    }

    pub fn insert(&mut self, key: K) -> bool {
        self.set.get_mut().insert(key)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.set.borrow().contains(key)
    }

    pub fn is_empty(&self, table: &impl KeyValidator<K>) -> bool {
        self.set.borrow_mut().retain(|k| table.is_key_valid(*k));
        self.set.borrow().is_empty()
    }

    pub fn remove(&mut self, key: &K) {
        self.set.get_mut().remove(key);
    }

    pub fn clear_invalid(&mut self, table: &impl KeyValidator<K>) {
        self.set.get_mut().retain(|k| table.is_key_valid(*k));
    }

    pub fn iter_valid(&self, table: &impl KeyValidator<K>) -> IntoIter<K> {
        let mut set = self.set.borrow_mut();
        set.retain(|k| table.is_key_valid(*k));
        let snapshot: Vec<_> = set.iter().copied().collect();
        snapshot.into_iter()
    }

    pub fn retain_valid(&mut self, table: &impl KeyValidator<K>, f: impl Fn(&K) -> bool) {
        self.set.get_mut().retain(|k| table.is_key_valid(*k) && f(k));
    }

    /// Consumes the set, returning every key, valid or not
    pub fn into_keys(self) -> impl Iterator<Item = K> {
        self.set.into_inner().into_iter()
    }

    pub fn drain_valid(&mut self, table: &impl KeyValidator<K>) -> IntoIter<K> {
        let valid: Vec<K> = self.set.get_mut().drain()
            .filter(|k| table.is_key_valid(*k))
            .collect();
        valid.into_iter()
    }
}

/// Map with weak keys
#[derive(Debug, Clone)]
pub struct WeakMap<K: Eq + Hash + Copy, V> {
    map: HashMap<K, V>,
}

impl<K: Eq + Hash + Copy, V> Default for WeakMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
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

    /// Returns the previous value of `key`, if any
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(key, value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key)
    }

    /// Whether at least one key is valid
    pub fn has_valid(&self, table: &impl KeyValidator<K>) -> bool {
        self.map.keys().any(|k| table.is_key_valid(*k))
    }

    pub fn clear_invalid(&mut self, table: &impl KeyValidator<K>) {
        self.map.retain(|k, _| table.is_key_valid(*k));
    }

    /// iterate over entries of valid keys. Invalid keys are skipped, not removed: see [`WeakMap::clear_invalid`]
    pub fn iter_valid<'a>(&'a self, table: &'a impl KeyValidator<K>) -> impl Iterator<Item = (K, &'a V)> {
        self.map.iter()
            .filter(|(k, _)| table.is_key_valid(**k))
            .map(|(k, v)| (*k, v))
    }

    /// iterate over values of valid keys
    pub fn iter_valid_values(&mut self, table: &impl KeyValidator<K>) -> impl Iterator<Item = &V> {
        self.map.retain(|k, _| table.is_key_valid(*k));
        self.map.values()
    }
}
