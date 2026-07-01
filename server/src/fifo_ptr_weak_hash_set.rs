use std::{collections::{HashSet, VecDeque}, hash::Hash};

use crate::core::symbols::symbol_keys::KeyValidator;

#[derive(Debug)]
pub struct FifoWeakHashSet<T: Copy + Eq + Hash> {
    set: HashSet<T>,
    queue: VecDeque<T>,
}

impl<T: Copy + Eq + Hash> Default for FifoWeakHashSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Eq + Hash> FifoWeakHashSet<T> {
    pub fn new() -> Self {
        Self {
            set: HashSet::default(),
            queue: VecDeque::new(),
        }
    }

    pub fn insert(&mut self, v: T) {
        if self.set.insert(v) {
            self.queue.push_back(v);
        }
    }

    /// Iterate over valid keys. This does not remove invalid keys from the set.
    pub fn iter_valid(&self, table: &impl KeyValidator<T>) -> impl Iterator<Item = T> {
        self.queue.iter().filter(|&&weak| table.is_key_valid(weak)).copied()
    }

    pub fn contains(&self, v: &T) -> bool {
        self.set.contains(v)
    }

    pub fn clear(&mut self) {
        self.set.clear();
        self.queue.clear();
    }

    pub fn remove(&mut self, v: &T) -> bool {
        if self.set.remove(v) {
            if let Some(pos) = self.queue.iter().position(|x| x == v) {
                self.queue.remove(pos);
            }
            return true
        }
        false
    }

    /// Returns true if the set is empty. Note that this does not consider
    /// whether the keys are valid or not, so it may return false even if all
    /// keys are expired.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Returns the number of elements in the set. Note that this does not consider
    /// whether the keys are valid or not, so it may return a non-zero value even if all
    /// keys are expired.
    pub fn len(&self) -> usize {
        self.set.len()
    }


    pub fn pop_front_valid(&mut self, table: &impl KeyValidator<T>) -> Option<T> {
        while let Some(v) = self.queue.pop_front() {
            self.set.remove(&v);
            if table.is_key_valid(v) {
                return Some(v)
            }
        }
        None
    }
}
