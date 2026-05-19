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

    pub fn is_empty(&self) -> bool {
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

    pub fn retain(&mut self, f: impl Fn(&K) -> bool) {
        self.set.get_mut().retain(|k| f(k));
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
