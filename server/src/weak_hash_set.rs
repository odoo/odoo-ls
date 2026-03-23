use std::{cell::RefCell, collections::HashSet, hash::Hash};

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

    pub fn iter_valid(&self, is_valid: impl Fn(&K) -> bool) -> std::vec::IntoIter<K> {
        let mut set = self.set.borrow_mut();
        set.retain(|k| is_valid(k));
        let snapshot: Vec<_> = set.iter().copied().collect();
        snapshot.into_iter()
    }

    pub fn retain(&mut self, f: impl Fn(&K) -> bool) {
        self.set.get_mut().retain(|k| f(k));
    }
}
