use std::{cell::RefCell, collections::{HashMap, HashSet, hash_map}, hash::Hash, vec::IntoIter};

#[derive(Debug, Clone)]
pub struct WeakSet<K: Eq + Hash + Copy> {
    set: RefCell<HashSet<K>>,
}

impl<K: Eq + Hash + Copy> WeakSet<K> {
    pub fn new() -> Self {
        Self {
            set: RefCell::new(HashSet::new()),
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

    pub fn clear_invalid(&mut self, is_valid: impl Fn(&K) -> bool) {
        self.set.get_mut().retain(|k| is_valid(k));
    }

    pub fn iter_valid(&self, is_valid: impl Fn(&K) -> bool) -> IntoIter<K> {
        let mut set = self.set.borrow_mut();
        set.retain(|k| is_valid(k));
        let snapshot: Vec<_> = set.iter().copied().collect();
        snapshot.into_iter()
    }

    pub fn retain(&mut self, f: impl Fn(&K) -> bool) {
        self.set.get_mut().retain(|k| f(k));
    }

    pub fn drain_valid(&mut self, is_valid: impl Fn(&K) -> bool) -> Vec<K> {
        let set = self.set.get_mut();
        let valid: Vec<K> = set.iter().copied().filter(|k| is_valid(k)).collect();
        set.clear();
        valid
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
            map: HashMap::new(),
        }
    }

    pub fn entry(&mut self, key: K) -> hash_map::Entry<'_, K, V> {
        self.map.entry(key)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key)
    }

    // @arena todo: take &impl ContainsKey instead of function
    /// iterate over values of valid keys
    pub fn iter_valid_values(&mut self, is_valid_key: impl Fn(&K) -> bool) -> IntoIter<&V> {
        self.map.retain(|k, _| is_valid_key(k));
        let snapshot: Vec<_> = self.map.values().collect();
        snapshot.into_iter()
    }
}
